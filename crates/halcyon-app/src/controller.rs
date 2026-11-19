//! Contrôleur : relie l'interface Slint à la bibliothèque, la base et les tâches de fond.
//!
//! Règle d'or : le thread de l'interface ne fait que des opérations courtes.
//! Analyse des sources, décodage d'images, téléchargements, succès et suivi des
//! sessions tournent dans des threads ; leurs résultats reviennent par `post()`.
//!
//! Le contrôleur est réparti en plusieurs fichiers (`ctl_*.rs`) par domaine.

use crate::ctl_bigpicture::BigPicture;
use crate::images::{self, ImageCache, ImageKey, Kind, LoadResult};
use crate::models::{card_for, detail_for, GridModel, LibraryData, ListModel, Tracking};
use crate::remote::RemoteImages;
use crate::{
    dialogs, Actions, AppUi, AppWindow, BpUi, CollectionItem, DetailUi, GameCard, HomeUi, LibraryUi, Nav,
    Page, SettingsUi, SourceInfo,
};
use halcyon_core::achievements::Progress;
use halcyon_core::util;
use halcyon_core::{
    Collection, Game, GameId, GameScanner, GameSource, LaunchSettingsUpdate, ManualGameInput, MetadataUpdate,
    ScanContext, SortKey, SourceFilter,
};
use halcyon_integrations::artwork::{url_candidates, Downloader, FetchError};
use halcyon_integrations::emulators::{Detected, Registry};
use halcyon_integrations::launcher::{self, LaunchOptions};
use halcyon_integrations::{epic, roms, standalone, steam, steam_web};
use halcyon_storage::{now_unix, AppPaths, ArtworkKind, Database, MergeStats, Settings};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

thread_local! {
    static CONTROLLER: RefCell<Option<Rc<Controller>>> = const { RefCell::new(None) };
}

/// Exécute `f` sur le thread de l'interface, avec le contrôleur.
pub fn post(f: impl FnOnce(&Rc<Controller>) + Send + 'static) {
    let _ = slint::invoke_from_event_loop(move || {
        let ctrl = CONTROLLER.with(|c| c.borrow().clone());
        if let Some(ctrl) = ctrl {
            f(&ctrl);
        }
    });
}

/// Nombre de jeux par étagère de l'accueil.
/// Les échecs de téléchargement ne sont pas retentés avant ce délai.
const RETRY_SECS: i64 = 7 * 86_400;
/// Fiches boutique Steam récupérées par analyse (limite de débit de la boutique).
const METADATA_PER_RUN: usize = 150;
/// Jeux interrogés sur SteamGridDB par passe (4 requêtes chacun au plus).
const SGDB_PER_RUN: usize = 60;
/// Recherches dans la boutique Steam par passage (elle limite à ~200 requêtes / 5 min).
const STORE_PER_RUN: usize = 40;
/// Jeux émulés cherchés dans libretro-thumbnails par passage.
const LIBRETRO_PER_RUN: usize = 80;

pub struct Controller {
    pub(crate) app: slint::Weak<AppWindow>,
    pub(crate) paths: AppPaths,
    pub(crate) db: Arc<Mutex<Database>>,
    pub(crate) settings: RefCell<Settings>,
    pub(crate) lib: Rc<RefCell<LibraryData>>,
    pub(crate) images: Rc<ImageCache>,
    pub(crate) remote: Rc<RemoteImages>,
    pub(crate) grid: Rc<GridModel>,
    pub(crate) list: Rc<ListModel>,
    pub(crate) shelves: [Rc<VecModel<GameCard>>; 3],
    pub(crate) detail_id: Cell<Option<GameId>>,
    pub(crate) hero_id: Cell<Option<GameId>>,
    /// Captures Steam affichées par le widget de l'accueil.
    pub(crate) home_captures: RefCell<Vec<steam::Screenshot>>,
    /// Jeu sélectionné dans la bibliothèque (clavier, manette).
    pub(crate) lib_selected: Cell<Option<GameId>>,
    /// Captures locales de la fiche affichée (jeu, liste).
    pub(crate) detail_captures: RefCell<(Option<GameId>, Vec<steam::Screenshot>)>,
    pub(crate) scanning: Cell<bool>,
    pub(crate) downloading: Arc<AtomicBool>,
    pub(crate) found: RefCell<HashMap<GameSource, (bool, String)>>,
    pub(crate) registry: RefCell<Registry>,
    pub(crate) detected: RefCell<HashMap<String, Detected>>,
    pub(crate) collections: RefCell<Vec<Collection>>,
    pub(crate) coll: RefCell<crate::ctl_collections::CollState>,
    pub(crate) uninstall: RefCell<crate::ctl_uninstall::UninstallState>,
    pub(crate) lb: RefCell<crate::ctl_launchbox::LbState>,
    pub(crate) genres: RefCell<Vec<String>>,
    /// Sources supplémentaires de l'index de la bibliothèque (autres launchers présents).
    pub(crate) extra_sources: RefCell<Vec<GameSource>>,
    pub(crate) progress: RefCell<HashMap<GameId, Progress>>,
    pub(crate) ach_syncing: Cell<bool>,
    pub(crate) poll_generation: Arc<AtomicU64>,
    pub(crate) active_session: Cell<Option<GameId>>,
    pub(crate) break_timer: RefCell<Option<slint::Timer>>,
    pub(crate) free_games: RefCell<Vec<halcyon_integrations::epic_free::FreeGame>>,
    pub(crate) presence: RefCell<halcyon_integrations::discord::Presence>,
    /// Jeu dont la configuration requise est affichée (évite de la redemander).
    pub(crate) req_shown: Cell<Option<GameId>>,
    pub(crate) bp: RefCell<BigPicture>,
    pub(crate) bp_clock: RefCell<Option<slint::Timer>>,
    pub(crate) pad_connected: Cell<bool>,
    pub(crate) smart: RefCell<Vec<halcyon_storage::SmartCollection>>,
    pub(crate) smart_edit: RefCell<crate::ctl_tracking::SmartEdit>,
    pub(crate) smart_rules: Rc<VecModel<crate::RuleRow>>,
    pub(crate) journal_cats: RefCell<Vec<bool>>,
    pub(crate) update_timer: RefCell<Option<slint::Timer>>,
    pub(crate) edit_companions: Rc<VecModel<crate::CompanionRow>>,
    pub(crate) editing_profile: RefCell<Option<halcyon_core::profiles::LaunchProfile>>,
    pub(crate) online: Cell<bool>,
    pub(crate) save_root_ok: Cell<bool>,
    /// Ne plus écrire les paramètres (restauration en cours).
    pub(crate) no_save: Cell<bool>,
    /// Emplacements de sauvegarde fournis par les extensions (nom compact du jeu → motifs).
    pub(crate) plugin_saves: RefCell<HashMap<String, Vec<String>>>,
    pub(crate) save_candidates: RefCell<Vec<halcyon_integrations::saves::Candidate>>,
    /// Base Ludusavi des emplacements de sauvegarde (chargée en arrière-plan).
    pub(crate) ludusavi: RefCell<Option<Arc<halcyon_integrations::ludusavi::Index>>>,
    /// Titres des objets du Steam Workshop déjà demandés (identifiant → titre).
    pub(crate) workshop_titles: RefCell<HashMap<u64, String>>,
    pub(crate) save_remote: RefCell<Option<String>>,
    pub(crate) mod_profile_cache: RefCell<Option<halcyon_storage::ModProfile>>,
    /// Extensions activées et valides.
    pub(crate) plugins: RefCell<Vec<halcyon_integrations::plugins::Manifest>>,
    pub(crate) plugin_updates: RefCell<HashMap<String, String>>,
    pub(crate) pads: RefCell<Vec<crate::gamepad::PadInfo>>,
    pub(crate) pad_edit: RefCell<Option<halcyon_storage::settings::PadProfile>>,
    pub(crate) remote_state: RefCell<crate::ctl_remote::RemoteState>,
    pub(crate) mc: RefCell<crate::ctl_minecraft::McState>,
    pub(crate) perfc: RefCell<crate::ctl_perf::PerfState>,
    pub(crate) downloads_timer: RefCell<Option<slint::Timer>>,
    pub(crate) news_cache: RefCell<HashMap<GameId, (i64, Vec<halcyon_integrations::news::NewsItem>)>>,
    pub(crate) tray: RefCell<Option<crate::tray::Tray>>,
    pub(crate) hotkey: RefCell<Option<(global_hotkey::GlobalHotKeyManager, global_hotkey::hotkey::HotKey)>>,
    pub(crate) system_timer: RefCell<Option<slint::Timer>>,
    pub(crate) mini: RefCell<Option<crate::MiniWindow>>,
    pub(crate) companion: RefCell<Option<crate::CompanionWindow>>,
    pub(crate) companion_timer: RefCell<Option<slint::Timer>>,
    pub(crate) companion_game: Cell<Option<GameId>>,
    pub(crate) hus: RefCell<crate::ctl_update::Hus>,
    pub(crate) history: RefCell<crate::nav::History<Page>>,
    pub(crate) tasks: RefCell<crate::tasks::TaskCenter>,
    /// Mode Performance automatique (partie en cours).
    pub(crate) perf_auto_active: Cell<bool>,
    /// Moteur audio (fil dédié, périphérique ouvert au premier son).
    pub(crate) audio: halcyon_audio::Audio,
    pub(crate) shell_timer: RefCell<Option<slint::Timer>>,
    #[allow(clippy::type_complexity)]
    pub(crate) pending_confirm: RefCell<Option<Box<dyn FnOnce(&Rc<Controller>)>>>,
}

impl Controller {
    pub fn install(app: &AppWindow, paths: AppPaths, db: Database, settings: Settings, games: Vec<Game>) -> Rc<Self> {
        let lib = Rc::new(RefCell::new(LibraryData { columns: 6, ..Default::default() }));
        let workers = std::thread::available_parallelism().map(|n| n.get().saturating_sub(1)).unwrap_or(2).clamp(1, 3);
        let images = Rc::new(ImageCache::new(paths.thumbs_dir(), workers, |key, result| {
            post(move |c| c.on_image_loaded(key, result));
        }));
        let remote = Rc::new(RemoteImages::new(paths.cache_dir.join("remote"), |url, ok| {
            post(move |c| c.on_remote_ready(url, ok));
        }));
        let audio = halcyon_audio::Audio::start(crate::ctl_sound::mix_from(&settings.sound));
        let ctrl = Rc::new(Self {
            app: app.as_weak(),
            db: Arc::new(Mutex::new(db)),
            settings: RefCell::new(settings),
            grid: Rc::new(GridModel::new(lib.clone(), images.clone())),
            list: Rc::new(ListModel::new(lib.clone(), images.clone())),
            lib,
            images,
            remote,
            shelves: [Rc::new(VecModel::default()), Rc::new(VecModel::default()), Rc::new(VecModel::default())],
            detail_id: Cell::new(None),
            hero_id: Cell::new(None),
            home_captures: RefCell::new(Vec::new()),
            lib_selected: Cell::new(None),
            detail_captures: RefCell::new((None, Vec::new())),
            scanning: Cell::new(false),
            downloading: Arc::new(AtomicBool::new(false)),
            found: RefCell::new(HashMap::new()),
            registry: RefCell::new(Registry::default()),
            detected: RefCell::new(HashMap::new()),
            collections: RefCell::new(Vec::new()),
            coll: RefCell::new(Default::default()),
            uninstall: RefCell::new(Default::default()),
            lb: RefCell::new(Default::default()),
            genres: RefCell::new(Vec::new()),
            extra_sources: RefCell::new(Vec::new()),
            progress: RefCell::new(HashMap::new()),
            ach_syncing: Cell::new(false),
            poll_generation: Arc::new(AtomicU64::new(0)),
            active_session: Cell::new(None),
            break_timer: RefCell::new(None),
            free_games: RefCell::new(Vec::new()),
            presence: RefCell::new(Default::default()),
            req_shown: Cell::new(None),
            bp: RefCell::new(BigPicture::default()),
            bp_clock: RefCell::new(None),
            pad_connected: Cell::new(false),
            smart: RefCell::new(Vec::new()),
            smart_edit: RefCell::new(Default::default()),
            smart_rules: Rc::new(VecModel::default()),
            journal_cats: RefCell::new(Vec::new()),
            update_timer: RefCell::new(None),
            edit_companions: Rc::new(VecModel::default()),
            editing_profile: RefCell::new(None),
            pending_confirm: RefCell::new(None),
            online: Cell::new(true),
            save_root_ok: Cell::new(true),
            no_save: Cell::new(false),
            plugin_saves: RefCell::new(HashMap::new()),
            save_candidates: RefCell::new(Vec::new()),
            ludusavi: RefCell::new(None),
            workshop_titles: RefCell::new(HashMap::new()),
            save_remote: RefCell::new(None),
            mod_profile_cache: RefCell::new(None),
            plugins: RefCell::new(Vec::new()),
            plugin_updates: RefCell::new(HashMap::new()),
            pads: RefCell::new(Vec::new()),
            pad_edit: RefCell::new(None),
            remote_state: RefCell::new(Default::default()),
            mc: RefCell::new(Default::default()),
            tray: RefCell::new(None),
            hotkey: RefCell::new(None),
            system_timer: RefCell::new(None),
            mini: RefCell::new(None),
            companion: RefCell::new(None),
            companion_timer: RefCell::new(None),
            companion_game: Cell::new(None),
            hus: RefCell::new(Default::default()),
            history: RefCell::new(crate::nav::History::new(Page::Home, 60)),
            tasks: RefCell::new(crate::tasks::TaskCenter::default()),
            perf_auto_active: Cell::new(false),
            perfc: RefCell::new(Default::default()),
            downloads_timer: RefCell::new(None),
            news_cache: RefCell::new(HashMap::new()),
            audio,
            shell_timer: RefCell::new(None),
            paths,
        });
        CONTROLLER.with(|c| *c.borrow_mut() = Some(ctrl.clone()));

        // Chronométrage du démarrage (journal détaillé : HALCYON_DEBUG=1).
        let mut sw = crate::perf::Stopwatch::new("démarrage");
        ctrl.load_plugins();
        sw.lap("extensions");
        ctrl.load_registry();
        ctrl.detect_emulators();
        sw.lap("émulateurs");
        ctrl.apply_appearance();
        ctrl.apply_ui_modes();
        ctrl.apply_pad_profile();
        sw.lap("apparence");
        ctrl.restore_ui_state(app);
        ctrl.lib.borrow_mut().set_games(games);
        sw.lap("bibliothèque en mémoire");
        ctrl.wire(app);
        sw.lap("câblage");
        ctrl.detect_sources();
        sw.lap("sources");
        if let Err(e) = ctrl.db().seed_smart_collections() {
            log::warn!("collections intelligentes par défaut non créées : {e}");
        }
        ctrl.reload_smart();
        ctrl.reload_collections();
        ctrl.reload_progress();
        sw.lap("collections et succès");
        ctrl.refresh_all();
        sw.lap("vues");
        // Pages secondaires : calculées à leur ouverture, ou juste après la première image
        // pour ce qui alimente la barre latérale (badges).
        ctrl.refresh_emulators_ui();
        sw.lap("émulateurs (badge)");
        sw.finish();
        ctrl
    }

    /// Tâches de démarrage, une fois la première image affichée.
    pub fn startup(self: &Rc<Self>) {
        self.apply_download_settings();
        // Badge « Mises à jour » et compteurs des collections intelligentes : différés après
        // l'affichage de la première image.
        self.refresh_updates();
        self.refresh_smart_ui();
        self.ensure_update_polling();
        self.start_connectivity_monitor();
        self.sync_pending_saves();
        self.start_ludusavi();
        self.announce_recap();
        self.check_free_games();
        self.check_disks();
        self.refresh_remote();
        self.ensure_tray();
        self.register_global_hotkey();
        self.start_system_timer();
        self.refresh_system_ui();
        // Le démarrage automatique enregistré par une autre version est resynchronisé.
        let auto = crate::desktop::autostart_enabled();
        if auto != self.settings.borrow().autostart {
            self.settings.borrow_mut().autostart = auto;
            self.save_settings();
        }
        let s = self.settings.borrow().clone();
        if s.start_in_big_picture {
            self.enter_big_picture();
        }
        if s.scan_on_startup {
            self.start_scan();
        } else if s.sync_achievements_on_startup {
            self.start_ach_sync(None, false);
        }
    }

    pub(crate) fn app(&self) -> Option<AppWindow> {
        self.app.upgrade()
    }

    pub(crate) fn db(&self) -> std::sync::MutexGuard<'_, Database> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn toast(&self, msg: impl Into<SharedString>, error: bool) {
        let msg: SharedString = msg.into();
        if error {
            self.sound(halcyon_audio::Cue::Error);
        }
        // Chaque bandeau est conservé dans le centre de commandes (les erreurs en particulier).
        self.task_event(if error { "error" } else { "notice" }, msg.as_str(), "", error, crate::tasks::Action::None);
        if let Some(app) = self.app() {
            let ui = app.global::<AppUi>();
            ui.set_toast(msg);
            ui.set_toast_error(error);
            ui.set_toast_visible(true);
        }
    }

    pub(crate) fn launch_options(&self) -> LaunchOptions {
        LaunchOptions { windows_runner: self.settings.borrow().windows_runner.clone() }
    }

    // ------------------------------------------------------------------ câblage

    fn restore_ui_state(&self, app: &AppWindow) {
        let s = self.settings.borrow();
        let lui = app.global::<LibraryUi>();
        let sort = SortKey::parse(&s.library_sort);
        lui.set_sort(SortKey::ALL.iter().position(|k| *k == sort).unwrap_or(0) as i32);
        lui.set_list_view(s.library_view == halcyon_storage::settings::LibraryView::List);
        lui.set_card_width(s.card_width as f32);
        drop(s);
        lui.set_view_mode(self.saved_view());
        let s = self.settings.borrow();
        lui.set_rows(ModelRc::from(self.grid.clone()));
        lui.set_items(ModelRc::from(self.list.clone()));
        drop(s);
        self.lib.borrow_mut().query = self.read_query(app);

        let home = app.global::<HomeUi>();
        home.set_recent(ModelRc::from(self.shelves[0].clone()));
        home.set_favorites(ModelRc::from(self.shelves[1].clone()));
        home.set_added(ModelRc::from(self.shelves[2].clone()));

        let ui = app.global::<AppUi>();
        ui.set_can_browse(dialogs::AVAILABLE);
        ui.set_scan_status(tr!("Jamais analysé pendant cette session").into());
    }

    fn wire(self: &Rc<Self>, app: &AppWindow) {
        self.wire_recap_with(app);
        let weak = Rc::downgrade(self);
        // Chaque callback récupère le contrôleur sans cycle de références.
        macro_rules! with {
            (|$c:ident $(, $arg:ident)*| $body:expr) => {{
                let weak = weak.clone();
                move |$($arg),*| {
                    if let Some($c) = weak.upgrade() {
                        $body
                    }
                }
            }};
        }
        macro_rules! with_ret {
            ($default:expr, |$c:ident $(, $arg:ident)*| $body:expr) => {{
                let weak = weak.clone();
                move |$($arg),*| weak.upgrade().map(|$c| $body).unwrap_or($default)
            }};
        }

        let actions = app.global::<Actions>();
        actions.on_open_game(with!(|c, id| c.open_game(id as GameId)));
        actions.on_play(with!(|c, id| c.play(id as GameId)));
        actions.on_toggle_favorite(with!(|c, id| c.toggle_favorite(id as GameId)));
        actions.on_toggle_hidden(with!(|c, id| c.toggle_hidden(id as GameId)));
        actions.on_open_folder(with!(|c, path| c.open_folder(&path)));
        actions.on_rescan(with!(|c| c.start_scan()));
        actions.on_open_url(with!(|c, url| c.open_url(&url)));
        actions.on_enter_big_picture(with!(|c| c.enter_big_picture()));
        actions.on_page_shown(with!(|c, page| c.on_page_shown(page)));
        actions.on_open_collections(with!(|c| c.open_collections_from_library()));

        let lui = app.global::<LibraryUi>();
        lui.on_query_changed(with!(|c| c.on_query_changed()));
        lui.on_columns_changed(with!(|c, n| c.on_columns_changed(n)));
        lui.on_view_changed(with!(|c| c.on_view_changed()));
        lui.on_move_selection(with!(|c, dx, dy| c.move_selection(dx, dy)));
        lui.on_select_index(with!(|c, i| c.select_index(i)));
        lui.on_activate_selection(with!(|c, play| c.activate_selection(play)));
        lui.on_card_size(with!(|c, px| c.set_card_size(px)));

        let dui = app.global::<DetailUi>();
        dui.on_save_launch(with!(|c, id, name, exe, args, wd| c.save_launch(id as GameId, &name, &exe, &args, &wd)));
        dui.on_save_metadata(with!(|c, id, d, dev, publ, rel, genres, tags| c.save_metadata(id as GameId, &d, &dev, &publ, &rel, &genres, &tags)));
        dui.on_set_status(with!(|c, id, s| c.set_status(id as GameId, s.max(0) as usize)));
        dui.on_save_notes(with!(|c, id, notes| c.save_notes(id as GameId, &notes)));
        dui.on_set_manual_playtime(with!(|c, id, v| c.set_manual_playtime(id as GameId, &v)));
        dui.on_play_with(with!(|c, id, p| c.play_from_detail(id as GameId, p)));
        dui.on_new_profile(with!(|c, id| c.new_profile(id as GameId)));
        dui.on_edit_profile(with!(|c, id| c.edit_profile(id as i64)));
        dui.on_save_profile(with!(|c, id, row| c.save_profile(id as GameId, row)));
        dui.on_delete_profile(with!(|c, id| c.delete_profile(id as i64)));
        dui.on_add_companion(with!(|c, path, args, close| c.add_companion(&path, &args, close)));
        dui.on_remove_companion(with!(|c, i| c.remove_companion(i.max(0) as usize)));
        dui.set_edit_companions(ModelRc::from(self.edit_companions.clone()));
        dui.on_save_add_location(with!(|c, id, p| c.add_save_location(id as GameId, &p)));
        dui.on_save_associate(with!(|c, id, i| c.associate_candidate(id as GameId, i.max(0) as usize)));
        dui.on_save_remove_location(with!(|c, id, i| c.remove_save_location(id as GameId, i.max(0) as usize)));
        dui.on_save_set_auto(with!(|c, id, b, a| c.set_save_auto(id as GameId, b, a)));
        dui.on_save_now(with!(|c, id, note| c.save_now(id as GameId, &note)));
        dui.on_save_restore(with!(|c, id, s| c.restore_snapshot(id as GameId, &s)));
        dui.on_save_compare(with!(|c, id, s| c.compare_snapshot(id as GameId, &s)));
        dui.on_save_delete(with!(|c, id, s| c.delete_snapshot(id as GameId, &s)));
        dui.on_save_keep_local(with!(|c, id| c.keep_local_saves(id as GameId)));
        dui.on_save_restore_remote(with!(|c, id| c.restore_remote(id as GameId)));
        dui.on_tools_refresh(with!(|c| c.refresh_game_tools()));
        dui.on_mod_toggle(with!(|c, id, m| c.toggle_mod(id as GameId, &m)));
        dui.on_mod_move(with!(|c, id, m, d| c.move_mod(id as GameId, &m, d)));
        dui.on_mod_import(with!(|c, id, p, n| c.import_mod(id as GameId, &p, &n)));
        dui.on_mod_delete(with!(|c, id, m| c.delete_mod(id as GameId, &m)));
        dui.on_mod_save_info(with!(|c, id, m, v, n| c.save_mod_info(id as GameId, &m, &v, &n)));
        dui.on_mod_set_adapter(with!(|c, id, i, t| c.set_mod_adapter(id as GameId, i.max(0) as usize, &t)));
        dui.on_mod_select_profile(with!(|c, id, i| c.select_mod_profile(id as GameId, i.max(0) as usize)));
        dui.on_mod_new_profile(with!(|c, id, n| c.new_mod_profile(id as GameId, &n)));
        dui.on_mod_delete_profile(with!(|c, id| c.delete_mod_profile(id as GameId)));
        dui.on_mod_deploy(with!(|c, id| c.deploy_mods(id as GameId)));
        dui.on_mod_undeploy(with!(|c, id| c.undeploy_mods(id as GameId)));
        dui.on_mod_check(with!(|c, id, d| c.check_mod_versions(id as GameId, &d)));

        let svui = app.global::<crate::SavesUi>();
        svui.on_set_root(with!(|c, p| c.set_save_root(&p)));
        svui.on_add_mirror(with!(|c, p| c.add_save_mirror(&p)));
        svui.on_remove_mirror(with!(|c, i| c.remove_save_mirror(i.max(0) as usize)));
        svui.on_set_keep(with!(|c, v| c.set_save_keep(&v)));
        svui.on_set_machine(with!(|c, v| c.set_machine_name(&v)));
        svui.on_detect_all(with!(|c| c.detect_all_saves()));
        svui.on_sync(with!(|c| c.sync_pending_saves()));

        let jui = app.global::<crate::JournalUi>();
        jui.on_toggle_category(with!(|c, i| c.toggle_journal_category(i.max(0) as usize)));
        jui.on_search_changed(with!(|c| c.refresh_journal()));
        jui.on_clear_filters(with!(|c| c.clear_journal_filters()));

        let uui = app.global::<crate::UpdatesUi>();
        uui.on_open_client(with!(|c, id| c.open_update_client(id as GameId)));
        uui.on_set_notify(with!(|c, v| c.set_update_notify(v)));
        uui.on_refresh(with!(|c| c.start_scan()));

        let smart = app.global::<crate::SmartUi>();
        smart.on_edit(with!(|c, id| c.smart_edit_existing(id as i64)));
        smart.on_create(with!(|c| c.smart_create()));
        smart.on_set_rule(with!(|c, i, k, p, n| c.smart_set_rule(i.max(0) as usize, k.max(0) as usize, &p, n)));
        smart.on_source_index(|key| GameSource::ALL.iter().position(|s| s.as_str() == key.as_str()).unwrap_or(0) as i32);
        smart.on_add_rule(with!(|c| c.smart_add_rule()));
        smart.on_remove_rule(with!(|c, i| c.smart_remove_rule(i.max(0) as usize)));
        smart.on_set_match_all(with!(|c, v| c.smart_set_match_all(v)));
        smart.on_save(with!(|c| c.smart_save()));
        smart.on_remove(with!(|c, id| c.smart_remove(id as i64)));
        smart.on_show(with!(|c, id| c.smart_show(id as i64)));
        dui.on_set_artwork(with!(|c, id, kind, path| c.set_custom_artwork(id as GameId, &kind, &path)));
        dui.on_set_emulator(with!(|c, id, index| c.set_game_emulator(id as GameId, index as usize)));
        dui.on_set_ra_id(with!(|c, id, value| c.set_ra_id(id as GameId, &value)));
        dui.on_remove(with!(|c, id| c.remove_game(id as GameId)));
        dui.on_toggle_collection(with!(|c, game, col| c.toggle_collection(game as GameId, col as i64)));
        dui.on_create_collection(with!(|c, game, name| c.create_collection(game as GameId, &name)));
        dui.on_sync_achievements(with!(|c, id| c.start_ach_sync(Some(id as GameId), true)));
        dui.on_add_custom_achievement(with!(|c, id, name, desc| c.add_custom_achievement(id as GameId, &name, &desc)));
        dui.on_toggle_custom_achievement(with!(|c, id| c.toggle_custom_achievement(id as i64)));
        dui.on_delete_custom_achievement(with!(|c, id| c.delete_custom_achievement(id as i64)));
        dui.on_browse_image(|| dialogs::pick_image().unwrap_or_default().into());

        let ach = app.global::<crate::AchUi>();
        ach.on_sync_all(with!(|c| c.start_ach_sync(None, true)));

        let emu = app.global::<crate::EmuUi>();
        emu.on_set_path(with!(|c, id, path| c.set_emulator_path(&id, &path)));
        emu.on_set_args(with!(|c, id, args| c.set_emulator_args(&id, &args)));
        emu.on_set_fullscreen(with!(|c, id, v| c.set_emulator_fullscreen(&id, v)));
        emu.on_set_default(with!(|c, platform, index| c.set_platform_default(&platform, index as usize)));
        emu.on_add_folder(with!(|c, index, path| c.add_rom_folder(index as usize, &path)));
        emu.on_remove_folder(with!(|c, platform, index| c.remove_rom_folder(&platform, index as usize)));
        emu.on_redetect(with!(|c| c.redetect_emulators()));
        emu.on_open_form(with!(|c, id| c.open_emulator_form(&id)));
        emu.on_toggle_platform(with!(|c, i, on| c.toggle_form_platform(i.max(0) as usize, on)));
        emu.on_save_form(with!(|c| c.save_emulator_form()));
        emu.on_delete_emulator(with!(|c, id| c.delete_custom_emulator(&id)));
        emu.on_add_platform(with!(|c, name, exts| c.add_custom_platform(&name, &exts)));
        emu.on_open_defs(with!(|c| c.open_folder(&c.paths.data_dir.join("emulators").to_string_lossy())));

        let bp = app.global::<BpUi>();
        bp.on_input(with!(|c, action| c.bp_input(&action)));

        let sui = app.global::<SettingsUi>();
        sui.on_set_source_enabled(with!(|c, key, v| c.set_source_enabled(&key, v)));
        sui.on_set_source_path(with!(|c, key, path| c.set_source_path(&key, &path)));
        sui.on_add_folder(with!(|c, path| c.add_folder(&path)));
        sui.on_remove_folder(with!(|c, i| c.remove_folder(i as usize)));
        sui.on_set_option(with!(|c, name, v| c.set_option(&name, v)));
        sui.on_launchbox(with!(|c, action| match action.as_str() {
            "on" => c.lb_set_enabled(true),
            "off" => c.lb_set_enabled(false),
            "build" => c.lb_build(),
            "fetch-all" => c.lb_fetch_all(),
            _ => c.lb_cancel(),
        }));
        sui.on_set_break_reminder(with!(|c, i| {
            c.settings.borrow_mut().break_reminder_hours = i.clamp(0, 3) as u32;
            c.save_settings();
            c.refresh_settings_ui();
        }));
        sui.on_set_language(with!(|c, i| c.set_ui_language(i)));
        sui.on_set_text(with!(|c, key, value| c.set_text(&key, &value)));
        sui.on_clear_cache(with!(|c| c.clear_cache()));
        sui.on_open_logs(with!(|c| c.open_folder(&c.paths.logs_dir.to_string_lossy())));
        sui.on_open_data(with!(|c| c.open_folder(&c.paths.data_dir.to_string_lossy())));
        sui.on_export_config(with!(|c, secrets, mods| c.export_config(secrets, mods)));
        sui.on_import_config(with!(|c, p| c.import_config(&p)));

        let ui = app.global::<AppUi>();
        ui.on_browse_file(|| dialogs::pick_file().unwrap_or_default().into());
        ui.on_browse_folder(|| dialogs::pick_folder().unwrap_or_default().into());
        ui.on_add_game(with_ret!(false, |c, name, exe, args, wd| c.add_manual_game(&name, &exe, &args, &wd)));
        ui.on_confirm_accepted(with!(|c| c.on_confirm_accepted()));

        let tui = app.global::<crate::ThemeUi>();
        tui.on_select_theme(with!(|c, i| c.select_theme(i.max(0) as usize)));
        tui.on_set_accent(with!(|c, v| c.set_accent(&v)));
        tui.on_set_density(with!(|c, d| c.set_density(d)));
        tui.on_set_animations(with!(|c, v| c.set_animations(v)));
        tui.on_set_background(with!(|c, p| c.set_background(&p)));
        tui.on_set_background_opacity(with!(|c, v| c.set_background_opacity(v)));
        tui.on_set_animated(with!(|c, v| c.set_animated_background(v)));
        tui.on_toggle_nav(with!(|c, k| c.toggle_nav(&k)));
        app.global::<SettingsUi>().on_search_changed(with!(|c, t| c.settings_search(&t)));
        tui.on_set_motion(with!(|c, m| c.set_motion(m)));
        tui.on_set_text_scale(with!(|c, v| c.set_text_scale(v)));
        tui.on_set_performance_auto(with!(|c, on| c.set_performance_auto(on)));
        tui.on_toggle_section(with!(|c, i| c.toggle_section(i.max(0) as usize)));
        tui.on_move_section(with!(|c, i, d| c.move_section(i.max(0) as usize, d)));
        tui.on_import_theme(with!(|c, p| c.import_theme(&p)));
        tui.on_export_theme(with!(|c| c.export_theme()));
        tui.on_reset(with!(|c| c.reset_appearance()));
        tui.on_save_profile(with!(|c, n| c.save_ui_profile(&n)));
        tui.on_apply_profile(with!(|c, i| c.apply_ui_profile(i.max(0) as usize)));
        tui.on_delete_profile(with!(|c, i| c.delete_ui_profile(i.max(0) as usize)));

        let pui = app.global::<crate::PluginsUi>();
        pui.on_install(with!(|c, p| c.install_plugin(&p)));
        pui.on_set_enabled(with!(|c, id, on| c.set_plugin_enabled(&id, on)));
        pui.on_uninstall(with!(|c, id| c.uninstall_plugin(&id)));
        pui.on_check_updates(with!(|c| c.check_plugin_updates()));
        pui.on_open_dir(with!(|c| c.open_plugins_dir()));

        let gui = app.global::<crate::PadsUi>();
        gui.on_set_active(with!(|c, i| c.set_active_pad_profile(i.max(0) as usize)));
        gui.on_new_profile(with!(|c| c.new_pad_profile()));
        gui.on_edit(with!(|c, id| c.edit_pad_profile(i64::from(id))));
        gui.on_save(with!(|c, name| c.save_pad_profile(&name)));
        gui.on_delete(with!(|c, id| c.delete_pad_profile(i64::from(id))));
        gui.on_capture_start(with!(|c, a| c.start_capture(&a)));
        gui.on_capture_cancel(with!(|c| c.cancel_capture()));
        gui.on_set_emu_arg(with!(|c, id, args| c.set_pad_emu_arg(&id, &args)));

        let rui = app.global::<crate::RemoteUi>();
        rui.on_refresh(with!(|c| c.refresh_remote()));
        rui.on_add_host(with!(|c, n, a| c.add_remote_host(&n, &a)));
        rui.on_remove_host(with!(|c, i| c.remove_remote_host(i.max(0) as usize)));
        rui.on_select_host(with!(|c, i| c.select_remote_host(i.max(0) as usize)));
        rui.on_pair(with!(|c, i| c.pair_remote(i.max(0) as usize)));
        rui.on_stream(with!(|c, i, a| c.stream_remote(i.max(0) as usize, &a)));
        rui.on_quit(with!(|c, i| c.quit_remote(i.max(0) as usize)));
        rui.on_open_sunshine(with!(|c| c.open_sunshine()));
        rui.on_set_moonlight_path(with!(|c, p| c.set_moonlight_path(&p)));

        app.global::<crate::StatsUi>().on_export(with!(|c, f| c.export_stats(&f)));

        let dl = app.global::<crate::DownloadsUi>();
        dl.on_set_paused(with!(|c, on| c.set_downloads_paused(on)));
        dl.on_set_limit(with!(|c, k| c.set_download_limit(k)));
        dl.on_set_concurrency(with!(|c, n| c.set_download_concurrency(n)));
        dl.on_cancel_minecraft(with!(|c| c.mc.borrow().cancel.store(true, Ordering::SeqCst)));

        let perf = app.global::<crate::PerfUi>();
        perf.on_reco_action(with!(|c, a| c.perf_reco_action(&a)));
        perf.on_set_history(with!(|c, on| c.set_perf_history(on)));
        perf.on_open_frame_log(with!(|c| c.perf_open_frame_log(false)));
        perf.on_open_latest_log(with!(|c| c.perf_open_frame_log(true)));

        let aui = app.global::<crate::AssistUi>();
        aui.on_refresh(with!(|c| c.refresh_assistant()));
        aui.on_surprise(with!(|c| c.surprise()));

        app.global::<DetailUi>().on_publish_sunshine(with!(|c, id| c.publish_sunshine(id as GameId)));

        let weak3 = weak.clone();
        app.window().on_close_requested(move || match weak3.upgrade() {
            Some(c) => c.on_close_requested(),
            None => slint::CloseRequestResponse::HideWindow,
        });
        self.wire_system(app);
        self.wire_hus(app);
        self.wire_shell(app);
        self.wire_home(app);
        self.wire_sound(app);
        self.wire_minecraft(app);
        self.wire_collections(app);
        self.wire_uninstall(app);
    }

    // ------------------------------------------------------------------ rafraîchissements

    /// Vues dépendant de la bibliothèque. Les pages non affichées sont recalculées à leur
    /// ouverture (`on_page_shown`) : rien n'est calculé pour une page invisible.
    pub(crate) fn refresh_all(&self) {
        let page = self.app().map(|a| a.global::<Nav>().get_page());
        let mut sw = crate::perf::Stopwatch::new("vues");
        self.refresh_filters();
        sw.lap("filtres");
        self.refresh_library();
        sw.lap("bibliothèque");
        if page == Some(Page::Home) {
            self.refresh_home();
            sw.lap("accueil");
        }
        if page == Some(Page::Settings) {
            self.refresh_settings_ui();
        }
        self.refresh_detail();
        sw.finish();
    }

    fn read_query(&self, app: &AppWindow) -> halcyon_core::LibraryQuery {
        let lui = app.global::<LibraryUi>();
        // Index : 0 = toutes, puis les collections manuelles, puis les intelligentes.
        let collection = {
            let manual = self.collections.borrow().len();
            match (lui.get_collection() as usize).checked_sub(1) {
                Some(i) if i < manual => {
                    let id = self.collections.borrow()[i].id;
                    self.db().collection_members(id).ok()
                }
                Some(i) => self.smart_members(i - manual),
                None => None,
            }
        };
        let genre = (lui.get_genre() as usize).checked_sub(1).and_then(|i| self.genres.borrow().get(i).cloned());
        halcyon_core::LibraryQuery {
            text: lui.get_search().to_string(),
            source: match lui.get_source() {
                1 => SourceFilter::Only(GameSource::Steam),
                2 => SourceFilter::Only(GameSource::Epic),
                3 => SourceFilter::Only(GameSource::Emulated),
                4 => SourceFilter::Only(GameSource::Standalone),
                5 => SourceFilter::Only(GameSource::Minecraft),
                6 => SourceFilter::Only(GameSource::Gog),
                n if n > 6 => self.extra_sources.borrow().get(n as usize - 7).map(|s| SourceFilter::Only(*s)).unwrap_or(SourceFilter::All),
                _ => SourceFilter::All,
            },
            installed_only: lui.get_installed_only(),
            favorites_only: lui.get_favorites_only(),
            include_hidden: lui.get_show_hidden(),
            sort: SortKey::ALL.get(lui.get_sort() as usize).copied().unwrap_or_default(),
            genre,
            collection,
        }
    }

    pub(crate) fn refresh_library(&self) {
        let Some(app) = self.app() else { return };
        {
            let mut lib = self.lib.borrow_mut();
            lib.ctx.progress = self.progress.borrow().clone();
            lib.ctx.platform_names = crate::models::platform_full_names();
        }
        let query = self.read_query(&app);
        let warning = {
            let q = halcyon_core::SearchQuery::parse(&query.text);
            if q.errors.is_empty() {
                String::new()
            } else {
                tr!("Filtre non reconnu : {join} (exemples : statut:terminé, temps:>10h, succès:manquants)", join = q.errors.join(", "))
            }
        };
        app.global::<LibraryUi>().set_search_warning(warning.into());
        let (count, total) = {
            let mut lib = self.lib.borrow_mut();
            lib.query = query;
            lib.apply_query();
            (lib.filtered.len(), lib.games.iter().filter(|g| !g.hidden).count())
        };
        let lui = app.global::<LibraryUi>();
        lui.set_result_count(count as i32);
        lui.set_total(total as i32);
        self.grid.reset();
        self.list.reset();
        self.sync_selection();
    }

    /// Listes des filtres « collection » et « genre ».
    pub(crate) fn refresh_filters(&self) {
        let Some(app) = self.app() else { return };
        let mut genres: Vec<String> = {
            let lib = self.lib.borrow();
            let set: HashSet<String> = lib.games.iter().flat_map(|g| g.genres.iter().cloned()).collect();
            set.into_iter().collect()
        };
        genres.sort_by_key(|g| util::sort_key(g));
        let lui = app.global::<LibraryUi>();
        let mut glabels: Vec<SharedString> = vec![tr!("Tous les genres").into()];
        glabels.extend(genres.iter().map(|g| SharedString::from(g.as_str())));
        // Conserve le genre sélectionné si possible.
        let current = (lui.get_genre() as usize).checked_sub(1).and_then(|i| self.genres.borrow().get(i).cloned());
        let new_index = current.and_then(|c| genres.iter().position(|g| *g == c)).map(|i| i + 1).unwrap_or(0);
        *self.genres.borrow_mut() = genres;
        lui.set_genre_labels(ModelRc::new(VecModel::from(glabels)));
        lui.set_genre(new_index as i32);

        // Autres launchers : affichés dans l'index seulement s'ils ont des jeux.
        let extra: Vec<GameSource> = {
            let lib = self.lib.borrow();
            GameSource::LAUNCHERS.into_iter().filter(|s| lib.games.iter().any(|g| g.source == *s)).collect()
        };
        let selected = (lui.get_source() as usize).checked_sub(7).and_then(|i| self.extra_sources.borrow().get(i).copied());
        lui.set_extra_sources(ModelRc::new(VecModel::from(extra.iter().map(|s| SharedString::from(s.label())).collect::<Vec<_>>())));
        if lui.get_source() > 6 {
            lui.set_source(selected.and_then(|s| extra.iter().position(|e| *e == s)).map(|i| i as i32 + 7).unwrap_or(0));
        }
        *self.extra_sources.borrow_mut() = extra;

        let counts = self.smart_counts();
        let cols = self.collections.borrow();
        let mut clabels: Vec<SharedString> = vec![tr!("Toutes les collections").into()];
        clabels.extend(cols.iter().map(|c| SharedString::from(format!("{} ({})", c.name, c.count))));
        clabels.extend(
            self.smart.borrow().iter().zip(counts).map(|(s, n)| SharedString::from(format!("⚡ {} ({n})", halcyon_i18n::t(&s.name)))),
        );
        let n = clabels.len();
        lui.set_collection_labels(ModelRc::new(VecModel::from(clabels)));
        if lui.get_collection() as usize >= n {
            lui.set_collection(0);
        }
    }

    /// Reprend les collections de Steam sous le nom « Steam · <nom> » : créées, tenues à jour
    /// (mêmes jeux que dans Steam) et retirées quand elles disparaissent de Steam. Les autres
    /// collections ne sont jamais touchées.
    fn sync_steam_collections(&self) {
        const PREFIX: &str = "Steam · ";
        if !self.settings.borrow().steam_enabled {
            return;
        }
        let Some(root) = steam::SteamScanner::new(self.settings.borrow().steam_path.clone()).root() else { return };
        let Some(account) = self.local_steam_id().map(|id| u64::from(halcyon_integrations::steam_stats::account_id(id))) else { return };
        // Fichier absent ou illisible : rien n'est créé ni retiré.
        let Some(wanted) = steam::collections(&root, account) else { return };
        if wanted.is_empty() && !self.collections.borrow().iter().any(|c| c.name.starts_with(PREFIX)) {
            return;
        }
        let by_appid: HashMap<String, GameId> =
            self.lib.borrow().games.iter().filter(|g| g.source == GameSource::Steam).map(|g| (g.source_key.clone(), g.id)).collect();
        let db = self.db();
        let existing = db.collections().unwrap_or_default();
        for (name, apps) in &wanted {
            let full = format!("{PREFIX}{name}");
            let id = match existing.iter().find(|c| c.name.eq_ignore_ascii_case(&full)) {
                Some(c) => c.id,
                None => match db.create_collection(&full) {
                    Ok(id) => id,
                    Err(e) => {
                        log::warn!("collection Steam « {name} » : {e}");
                        continue;
                    }
                },
            };
            let want: HashSet<GameId> = apps.iter().filter_map(|a| by_appid.get(&a.to_string()).copied()).collect();
            let have = db.collection_members(id).unwrap_or_default();
            for g in want.difference(&have) {
                let _ = db.set_in_collection(id, *g, true);
            }
            for g in have.difference(&want) {
                let _ = db.set_in_collection(id, *g, false);
            }
        }
        for c in existing.iter().filter(|c| c.name.starts_with(PREFIX)) {
            if !wanted.iter().any(|(n, _)| c.name.eq_ignore_ascii_case(&format!("{PREFIX}{n}"))) {
                let _ = db.delete_collection(c.id);
            }
        }
        if !wanted.is_empty() {
            log::info!("Steam : {} collections reprises", wanted.len());
        }
    }

    pub(crate) fn reload_collections(&self) {
        let cols = self.db().collections().unwrap_or_default();
        *self.collections.borrow_mut() = cols;
    }

    /// « Steam · 12 h 30 de jeu · Joué hier · 12/40 succès »
    pub(crate) fn meta_line(&self, g: &Game, now: i64) -> String {
        let mut meta = vec![g.source.label().to_string()];
        if g.effective_playtime() > 0 {
            meta.push(tr!("{effective_playtime} de jeu", effective_playtime = util::format_playtime(g.effective_playtime())));
        }
        if let Some(t) = g.last_played {
            meta.push(tr!("Joué : {t}", t = util::format_relative_inline(t, now)));
        }
        if let Some(p) = self.progress.borrow().get(&g.id) {
            meta.push(trn!(p.total, "{unlocked}/{n} succès", "{unlocked}/{n} succès", unlocked = halcyon_i18n::number(p.unlocked as u64)));
        }
        meta.join("  ·  ")
    }

    pub(crate) fn refresh_detail(&self) {
        let (Some(app), Some(id)) = (self.app(), self.detail_id.get()) else { return };
        let game = { self.lib.borrow().get(id).cloned() };
        let Some(g) = game else { return };
        // Un seul verrou à la fois : les temporaires vivent jusqu'à la fin de l'instruction.
        let sessions = self.db().session_summary(id).unwrap_or((0, 0));
        let first_session = self.db().session_bounds(id).ok().flatten().map(|b| b.0);
        let tracking = Tracking { sessions, first_session };
        let mut detail = detail_for(&g, &self.images, tracking, now_unix());
        if g.source == GameSource::Emulated {
            detail.launch_mode = match self.emulator_for(&g) {
                Some((def, _)) => tr!("Émulateur : {name}", name = def.name),
                None => tr!("Aucun émulateur détecté pour cette plateforme"),
            }
            .into();
        }
        let dui = app.global::<DetailUi>();
        dui.set_game(detail);

        self.publish_detail_shots(&g);
        // Fond d'ambiance (pas de bannière) et captures locales.
        if g.artwork.banner.is_none() {
            let amb = self.images.get(ImageKey { game: g.id, kind: Kind::Ambient }, g.artwork.cover.as_deref());
            dui.set_ambient(amb.unwrap_or_default());
        }
        self.load_detail_captures(&g);

        let member = self.db().game_collections(id).unwrap_or_default();
        let items: Vec<CollectionItem> = self
            .collections
            .borrow()
            .iter()
            .map(|c| CollectionItem { id: c.id as i32, name: c.name.as_str().into(), member: member.contains(&c.id), count: c.count as i32 })
            .collect();
        dui.set_collections(ModelRc::new(VecModel::from(items)));

        let (options, index) = self.emulator_options(&g);
        dui.set_emulator_options(ModelRc::new(VecModel::from(options)));
        dui.set_emulator_index(index as i32);
        self.refresh_detail_achievements(&g);
        self.refresh_tracking_ui(&g);
        self.refresh_profiles_ui(&g);
        dui.set_links(ModelRc::new(VecModel::from(self.plugin_links(&g))));
        dui.set_can_publish(self.can_publish() && g.installed);
        self.refresh_requirements(&g);
    }

    /// Visuels de la fiche (captures distantes), republiés au fil des téléchargements.
    fn publish_detail_shots(&self, g: &Game) {
        let Some(app) = self.app() else { return };
        let dui = app.global::<DetailUi>();
        let online = self.settings.borrow().online_metadata;
        let shots: Vec<slint::Image> = g
            .screenshots
            .iter()
            .filter_map(|url| {
                let path = self.remote.ensure(url, online && self.net_allowed())?;
                self.images.get(ImageKey::remote(url, Kind::Shot), Some(&path))
            })
            .collect();
        dui.set_screenshots(ModelRc::new(VecModel::from(shots)));
        dui.set_slides(ModelRc::new(VecModel::from(self.banner_slides(g))));
    }

    /// Une image distante est arrivée sur la fiche : seules les listes d'images sont
    /// republiées (pas de requêtes ni de recalcul du reste de la page).
    pub(crate) fn refresh_detail_media(&self) {
        let Some(id) = self.detail_id.get() else { return };
        let game = { self.lib.borrow().get(id).cloned() };
        let Some(g) = game else { return };
        self.publish_detail_shots(&g);
        self.publish_detail_captures();
        self.refresh_detail_achievements(&g);
    }

    /// Met à jour toutes les vues d'un jeu modifié.
    pub(crate) fn game_changed(&self, id: GameId) {
        self.grid.game_changed(id);
        self.list.game_changed(id);
        self.strip_game_changed(id);
        let card = {
            let lib = self.lib.borrow();
            lib.get(id).map(|g| card_for(g, &self.images, now_unix()))
        };
        if let Some(card) = card {
            for shelf in &self.shelves {
                for i in 0..shelf.row_count() {
                    if shelf.row_data(i).is_some_and(|c| c.id == card.id) {
                        shelf.set_row_data(i, card.clone());
                    }
                }
            }
        }
        if self.hero_id.get() == Some(id) {
            let g = { self.lib.borrow().get(id).cloned() };
            if let (Some(app), Some(g)) = (self.app(), g) {
                app.global::<HomeUi>().set_hero(detail_for(&g, &self.images, Tracking::default(), now_unix()));
            }
        }
        if self.detail_id.get() == Some(id) {
            self.refresh_detail();
        }
        if self.bp.borrow().active {
            self.refresh_bp();
        }
    }

    // ------------------------------------------------------------------ événements interface

    fn on_query_changed(&self) {
        self.refresh_library();
        let sort = self.lib.borrow().query.sort;
        let changed = {
            let mut s = self.settings.borrow_mut();
            let changed = s.library_sort != sort.as_str();
            s.library_sort = sort.as_str().to_string();
            changed
        };
        if changed {
            self.save_settings();
        }
    }

    fn on_columns_changed(&self, n: i32) {
        let n = n.max(1) as usize;
        if self.lib.borrow().columns != n {
            self.lib.borrow_mut().columns = n;
            self.grid.reset();
        }
    }

    fn on_image_loaded(&self, key: ImageKey, result: LoadResult) {
        if self.images.complete(key, result) {
            match key.kind {
                Kind::Cover => self.game_changed(key.game),
                Kind::Banner | Kind::Logo => {
                    // Logo ou bannière d'une collection (clé dérivée du chemin).
                    if key.game >= (1 << 60) {
                        self.collection_image_loaded();
                    }
                    if self.detail_id.get() == Some(key.game) {
                        self.refresh_detail();
                    }
                    if self.hero_id.get() == Some(key.game) || self.bp.borrow().active {
                        self.game_changed(key.game);
                    }
                }
                Kind::Icon | Kind::Shot | Kind::Slide => self.refresh_remote_views(),
                Kind::CoverLarge | Kind::Spine => {
                    if self.bp.borrow().active {
                        self.refresh_bp();
                    }
                }
                Kind::Ambient => {
                    if self.bp.borrow().active {
                        self.refresh_bp();
                    }
                    self.strip_game_changed(key.game);
                    if self.detail_id.get() == Some(key.game) {
                        self.refresh_detail();
                    }
                }
            }
        }
    }

    pub(crate) fn on_page_shown(&self, page: Page) {
        // Mesures en direct : uniquement tant que le Performance Center est affiché.
        if page != Page::Performance {
            self.perf_close();
        }
        if page != Page::Downloads {
            self.downloads_close();
        }
        match page {
            Page::Downloads => self.downloads_open(),
            Page::Performance => self.perf_open(),
            Page::Achievements => self.refresh_ach_page(),
            Page::Emulators => self.refresh_emulators_ui(),
            Page::Settings => {
                self.refresh_settings_ui();
                self.refresh_system_ui();
                self.refresh_language_ui();
            }
            Page::Home => self.refresh_home(),
            Page::Stats => self.refresh_stats(),
            Page::Journal => self.refresh_journal(),
            Page::Recap => self.refresh_recap(),
            Page::Storage => self.refresh_storage(),
            Page::Collections => self.refresh_collections_page(),
            Page::Updates => self.refresh_updates(),
            Page::Saves => self.refresh_saves_page(),
            Page::Appearance => self.refresh_appearance_ui(),
            Page::Plugins => self.refresh_plugins_ui(),
            Page::Controllers => self.refresh_pads_ui(),
            Page::Remote => self.refresh_remote(),
            Page::Assistant => self.refresh_assistant(),
            Page::Minecraft => self.mc_open(),
            _ => {}
        }
    }

    fn on_remote_ready(&self, url: String, ok: bool) {
        self.remote.finished(&url);
        if ok {
            self.refresh_remote_views();
        }
    }

    /// Vues contenant des images distantes (icônes de succès, captures).
    fn refresh_remote_views(&self) {
        if let Some(app) = self.app() {
            match app.global::<Nav>().get_page() {
                Page::Game => self.refresh_detail_media(),
                Page::Achievements => self.refresh_ach_page(),
                Page::Home => self.publish_captures(),
                _ => {}
            }
        }
        if self.bp.borrow().active {
            self.refresh_bp();
        }
    }

    // ------------------------------------------------------------------ actions sur les jeux

    pub(crate) fn open_game(&self, id: GameId) {
        self.sound(halcyon_audio::Cue::GameOpen);
        self.push_game_history(id);
        self.show_game(id);
    }

    /// Affiche une fiche sans toucher à l'historique (précédent / suivant).
    pub(crate) fn show_game(&self, id: GameId) {
        let Some(app) = self.app() else { return };
        self.detail_id.set(Some(id));
        app.global::<DetailUi>().set_tab(0);
        app.global::<DetailUi>().set_show_hidden_ach(false);
        app.global::<DetailUi>().set_editing_open(false);
        app.global::<DetailUi>().set_profile_notes(ModelRc::default());
        app.global::<DetailUi>().set_save_diff_title("".into());
        app.global::<DetailUi>().set_save_diff(ModelRc::default());
        self.refresh_detail();
        self.refresh_game_tools();
        self.refresh_game_news(id);
        let nav = app.global::<Nav>();
        if nav.get_page() != Page::Game {
            nav.set_page(Page::Game);
        }
    }

    pub(crate) fn mark_played(&self, id: GameId, at: i64) {
        if let Some(g) = self.lib.borrow_mut().get_mut(id) {
            g.last_played = Some(at);
            g.play_count += 1;
        }
        self.game_changed(id);
        self.refresh_home();
    }

    pub(crate) fn minimize_if_enabled(&self) {
        if self.settings.borrow().minimize_on_launch {
            if let Some(app) = self.app() {
                app.window().set_minimized(true);
            }
        }
    }

    pub(crate) fn toggle_favorite(&self, id: GameId) {
        let Some(fav) = self.lib.borrow().get(id).map(|g| !g.favorite) else { return };
        if let Err(e) = self.db().set_favorite(id, fav) {
            self.toast(tr!("Erreur : {e}", e = e), true);
            return;
        }
        if let Some(g) = self.lib.borrow_mut().get_mut(id) {
            g.favorite = fav;
        }
        self.sound(if fav { halcyon_audio::Cue::Favorite } else { halcyon_audio::Cue::Setting });
        if self.lib.borrow().query.favorites_only {
            self.refresh_library();
        } else {
            self.game_changed(id);
        }
        self.refresh_home();
    }

    pub(crate) fn toggle_hidden(&self, id: GameId) {
        let Some((hidden, name)) = self.lib.borrow().get(id).map(|g| (!g.hidden, g.name.clone())) else { return };
        if let Err(e) = self.db().set_hidden(id, hidden) {
            self.toast(tr!("Erreur : {e}", e = e), true);
            return;
        }
        if let Some(g) = self.lib.borrow_mut().get_mut(id) {
            g.hidden = hidden;
        }
        self.refresh_library();
        self.refresh_home();
        self.refresh_detail();
        self.toast(
            if hidden {
                tr!("« {name} » est masqué (filtre « Masqués » pour le retrouver)", name = name)
            } else {
                tr!("« {name} » est de nouveau visible", name = name)
            },
            false,
        );
    }

    pub(crate) fn open_folder(&self, path: &str) {
        let path = Path::new(path);
        if path.as_os_str().is_empty() {
            self.toast(tr!("Aucun dossier associé"), true);
            return;
        }
        if !path.exists() {
            if let Err(e) = std::fs::create_dir_all(path) {
                self.toast(tr!("Ce dossier n'existe pas : {e}", e = e), true);
                return;
            }
        }
        if let Err(e) = launcher::open_path(path) {
            self.toast(tr!("Ouverture impossible : {e}", e = e), true);
        }
    }

    pub(crate) fn open_url(&self, url: &str) {
        if !url.starts_with("https://") {
            return;
        }
        if let Err(e) = launcher::open_uri(url) {
            self.toast(tr!("Ouverture impossible : {e}", e = e), true);
        }
    }

    pub(crate) fn replace_game(&self, game: Game) {
        self.lib.borrow_mut().upsert(game);
        self.refresh_filters();
        self.refresh_library();
        self.refresh_home();
        self.refresh_detail();
    }

    fn save_launch(&self, id: GameId, name: &str, exe: &str, args: &str, wd: &str) {
        let Some(current) = self.lib.borrow().get(id).cloned() else { return };
        let opt = |s: &str| Some(PathBuf::from(s.trim())).filter(|p| !p.as_os_str().is_empty());
        // Jeux émulés : seul le nom est modifiable ici (le lancement dépend de l'émulateur).
        let update = if current.source == GameSource::Emulated {
            LaunchSettingsUpdate {
                name: name.to_string(),
                executable: current.executable.clone(),
                arguments: current.arguments.clone(),
                working_dir: current.working_dir.clone(),
            }
        } else {
            LaunchSettingsUpdate { name: name.to_string(), executable: opt(exe), arguments: args.to_string(), working_dir: opt(wd) }
        };
        if current.source != GameSource::Emulated {
            if let Some(exe) = &update.executable {
                if !exe.is_file() {
                    self.toast(tr!("Exécutable introuvable : {exe}", exe = exe.display()), true);
                    return;
                }
            }
            if let Some(wd) = &update.working_dir {
                if !wd.is_dir() {
                    self.toast(tr!("Répertoire introuvable : {wd}", wd = wd.display()), true);
                    return;
                }
            }
        }
        let result = self.db().update_launch_settings(id, &update);
        match result {
            Ok(game) => {
                self.replace_game(game);
                self.toast(tr!("Paramètres enregistrés"), false);
            }
            Err(e) => self.toast(tr!("Enregistrement impossible : {e}", e = e), true),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn save_metadata(
        &self,
        id: GameId,
        description: &str,
        developer: &str,
        publisher: &str,
        release: &str,
        genres: &str,
        tags: &str,
    ) {
        let list = |s: &str| -> Vec<String> { s.split([',', ';']).map(|g| g.trim().to_string()).filter(|g| !g.is_empty()).collect() };
        let genres = list(genres);
        let update = MetadataUpdate {
            description: description.to_string(),
            developer: developer.to_string(),
            publisher: publisher.to_string(),
            release_date: release.to_string(),
            genres,
            tags: list(tags),
        };
        let result = self.db().update_metadata(id, &update);
        match result {
            Ok(game) => {
                self.replace_game(game);
                self.toast(tr!("Métadonnées enregistrées"), false);
            }
            Err(e) => self.toast(tr!("Enregistrement impossible : {e}", e = e), true),
        }
    }

    /// Copie une image choisie par l'utilisateur dans les données de Halcyon.
    fn set_custom_artwork(&self, id: GameId, kind: &str, path: &str) {
        let src = PathBuf::from(path.trim().trim_matches('"'));
        let kind = match kind {
            "cover" => ArtworkKind::Cover,
            "banner" => ArtworkKind::Banner,
            _ => return,
        };
        if image::ImageReader::open(&src).and_then(|r| r.with_guessed_format()).map(|r| r.format().is_none()).unwrap_or(true) {
            self.toast(tr!("Image illisible : {src}", src = src.display()), true);
            return;
        }
        let dir = self.paths.data_dir.join("custom-artwork");
        let ext = src.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_else(|| "img".into());
        let dest = dir.join(format!("{id}-{}-{}.{ext}", if kind == ArtworkKind::Cover { "cover" } else { "banner" }, now_unix()));
        let result = std::fs::create_dir_all(&dir).and_then(|_| std::fs::copy(&src, &dest));
        if let Err(e) = result {
            self.toast(tr!("Copie impossible : {e}", e = e), true);
            return;
        }
        if let Err(e) = self.db().set_artwork(id, kind, Some(&dest), true) {
            self.toast(tr!("Enregistrement impossible : {e}", e = e), true);
            return;
        }
        if let Some(g) = self.lib.borrow_mut().get_mut(id) {
            match kind {
                ArtworkKind::Cover => g.artwork.cover = Some(dest),
                _ => g.artwork.banner = Some(dest),
            }
        }
        self.images.invalidate(id);
        self.game_changed(id);
        self.toast(tr!("Visuel mis à jour"), false);
    }

    pub(crate) fn remove_game(&self, id: GameId) {
        let result = self.db().remove_manual_game(id);
        match result {
            Ok(()) => {
                self.lib.borrow_mut().remove(id);
                self.history.borrow_mut().forget_game(id);
                self.images.invalidate(id);
                self.detail_id.set(None);
                if let Some(app) = self.app() {
                    app.global::<Nav>().invoke_back();
                }
                self.refresh_library();
                self.refresh_home();
                self.toast(tr!("Jeu retiré de la bibliothèque"), false);
            }
            Err(e) => self.toast(e.to_string(), true),
        }
    }

    fn add_manual_game(&self, name: &str, exe: &str, args: &str, wd: &str) -> bool {
        let exe_path = PathBuf::from(exe.trim().trim_matches('"'));
        if !exe_path.is_file() {
            self.toast(tr!("Fichier introuvable : {exe_path}", exe_path = exe_path.display()), true);
            return false;
        }
        let wd = Some(PathBuf::from(wd.trim())).filter(|p| !p.as_os_str().is_empty());
        if let Some(wd) = &wd {
            if !wd.is_dir() {
                self.toast(tr!("Répertoire introuvable : {wd}", wd = wd.display()), true);
                return false;
            }
        }
        let input = ManualGameInput { name: name.to_string(), executable: exe_path, arguments: args.to_string(), working_dir: wd };
        let result = self.db().add_manual(&input);
        match result {
            Ok(game) => {
                let msg = tr!("« {name} » a été ajouté", name = game.name);
                self.lib.borrow_mut().upsert(game);
                self.refresh_library();
                self.refresh_home();
                self.toast(msg, false);
                true
            }
            Err(e) => {
                self.toast(e.to_string(), true);
                false
            }
        }
    }

    fn toggle_collection(&self, game: GameId, collection: i64) {
        let member = self.db().game_collections(game).map(|s| s.contains(&collection)).unwrap_or(false);
        if let Err(e) = self.db().set_in_collection(collection, game, !member) {
            self.toast(tr!("Erreur : {e}", e = e), true);
            return;
        }
        self.sound(if member { halcyon_audio::Cue::Setting } else { halcyon_audio::Cue::Collection });
        self.reload_collections();
        self.refresh_filters();
        self.refresh_library();
        self.refresh_detail();
    }

    fn create_collection(&self, game: GameId, name: &str) {
        let result = {
            let db = self.db();
            db.create_collection(name).and_then(|c| db.set_in_collection(c, game, true))
        };
        match result {
            Ok(()) => {
                self.reload_collections();
                self.refresh_filters();
                self.refresh_detail();
                self.sound(halcyon_audio::Cue::Collection);
                self.toast(tr!("Ajouté à la collection « {name} »", name = name.trim()), false);
            }
            Err(e) => self.toast(e.to_string(), true),
        }
    }

    // ------------------------------------------------------------------ analyse des sources

    /// Détecte (sans analyser) les sources disponibles, pour la page Paramètres.
    pub(crate) fn detect_sources(&self) {
        let s = self.settings.borrow();
        let steam = steam::SteamScanner::new(s.steam_path.clone()).root();
        let epic = epic::EpicScanner::new(s.epic_manifests_path.clone()).location();
        let mut found = self.found.borrow_mut();
        found.insert(GameSource::Steam, (steam.is_some(), steam.map(|p| p.display().to_string()).unwrap_or_default()));
        found.insert(GameSource::Epic, (epic.is_some(), epic.map(|p| p.display().to_string()).unwrap_or_default()));
        let gog = halcyon_integrations::gog::GogScanner::new(s.gog_heroic_path.clone()).location();
        found.insert(GameSource::Gog, (gog.is_some(), gog.map(|p| p.display().to_string()).unwrap_or_default()));
        for l in GameSource::LAUNCHERS {
            let at = halcyon_integrations::launchers::LauncherScanner::location(l);
            found.insert(l, (at.is_some(), at.unwrap_or_default()));
        }
    }

    pub(crate) fn start_scan(&self) {
        if self.scanning.replace(true) {
            return;
        }
        if let Some(app) = self.app() {
            let ui = app.global::<AppUi>();
            ui.set_scanning(true);
            ui.set_scan_status(tr!("Préparation…").into());
        }
        self.task_start("scan", tr!("Analyse des sources"));
        let mut settings = self.settings.borrow().clone();
        for f in self.plugin_game_folders() {
            if !settings.scan_folders.contains(&f) {
                settings.scan_folders.push(f);
            }
        }
        let steam_web = if self.net_allowed() { self.steam_web() } else { None };
        let tags_cache = self.paths.cache_dir.join(format!("steam-tags-{}.json", steam_web::steam_language()));
        let tags_online = self.net_allowed() && settings.online_metadata;
        let rom_folders: Vec<_> = {
            let reg = self.registry.borrow();
            settings
                .rom_folders
                .iter()
                .filter_map(|f| reg.platform(&f.platform).map(|p| (p.clone(), f.path.clone())))
                .collect()
        };
        let db = self.db.clone();
        let mc_layout = self.mc_layout();
        let started = std::time::Instant::now();
        std::thread::Builder::new()
            .name("halcyon-scan".into())
            .spawn(move || {
                let progress: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(|msg: &str| {
                    let msg = msg.to_string();
                    post(move |c| c.set_scan_status(&msg));
                });
                let ctx = ScanContext::new(Arc::new(AtomicBool::new(false)), progress);
                let mut scanners: Vec<Box<dyn GameScanner>> = Vec::new();
                if settings.steam_enabled {
                    let tags = Arc::new(steam_tag_names(&tags_cache, tags_online));
                    scanners.push(Box::new(steam::SteamScanner::new(settings.steam_path.clone()).with_web(steam_web).with_tag_names(tags)));
                }
                if settings.epic_enabled {
                    scanners.push(Box::new(epic::EpicScanner::new(settings.epic_manifests_path.clone())));
                }
                if settings.gog_enabled {
                    scanners.push(Box::new(halcyon_integrations::gog::GogScanner::new(settings.gog_heroic_path.clone())));
                }
                for l in GameSource::LAUNCHERS {
                    use halcyon_integrations::launchers::LauncherScanner;
                    if LauncherScanner::supported(l) && !settings.disabled_launchers.iter().any(|d| d == l.as_str()) {
                        scanners.push(Box::new(LauncherScanner::new(l)));
                    }
                }
                // Toujours exécutés : retirer un dossier rend ses jeux « non installés ».
                scanners.push(Box::new(standalone::StandaloneScanner::new(settings.scan_folders.clone())));
                scanners.push(Box::new(roms::RomScanner::new(rom_folders)));
                scanners.push(Box::new(crate::ctl_minecraft::McScanner { layout: mc_layout }));

                let mut results: Vec<(GameSource, Result<MergeStats, String>)> = Vec::new();
                for scanner in scanners {
                    let source = scanner.source();
                    let outcome = match scanner.scan(&ctx) {
                        Ok(report) => {
                            for w in &report.warnings {
                                log::warn!("{} : {w}", source.label());
                            }
                            db.lock().unwrap_or_else(|e| e.into_inner()).merge_scan(source, &report).map_err(|e| e.to_string())
                        }
                        Err(e) => Err(e.to_string()),
                    };
                    match &outcome {
                        Ok(s) => log::info!("{} : +{} / ~{} / désinstallés {}", source.label(), s.added, s.updated, s.uninstalled),
                        Err(e) => log::info!("{} ignoré : {e}", source.label()),
                    }
                    results.push((source, outcome));
                }
                let games = db.lock().unwrap_or_else(|e| e.into_inner()).load_games();
                let elapsed = started.elapsed();
                post(move |c| c.on_scan_finished(games, results, elapsed));
            })
            .expect("création du thread d'analyse");
    }

    fn set_scan_status(&self, msg: &str) {
        let scan = self.tasks.borrow().find_kind("scan");
        if let Some(id) = scan {
            self.tasks.borrow_mut().update(id, None, Some(msg));
        }
        self.publish_tasks();
        if let Some(app) = self.app() {
            app.global::<AppUi>().set_scan_status(msg.into());
        }
    }

    /// Remplace la bibliothèque en mémoire par celle de la base.
    pub(crate) fn reload_games(&self, games: Vec<Game>) {
        {
            let lib = self.lib.borrow();
            for g in &games {
                if lib.get(g.id).is_some_and(|old| old.artwork != g.artwork) {
                    self.images.invalidate(g.id);
                }
            }
        }
        self.lib.borrow_mut().set_games(games);
    }

    fn on_scan_finished(
        self: &Rc<Self>,
        games: halcyon_storage::Result<Vec<Game>>,
        results: Vec<(GameSource, Result<MergeStats, String>)>,
        elapsed: std::time::Duration,
    ) {
        self.scanning.set(false);
        let games = match games {
            Ok(g) => g,
            Err(e) => {
                log::error!("relecture de la bibliothèque impossible : {e}");
                self.task_finish_kind("scan", false, tr!("Erreur de base de données : {e}", e = e));
                self.toast(tr!("Erreur de base de données : {e}", e = e), true);
                if let Some(app) = self.app() {
                    app.global::<AppUi>().set_scanning(false);
                }
                return;
            }
        };
        self.reload_games(games);
        self.detect_sources();
        self.sync_steam_collections();
        self.reload_collections();
        self.refresh_all();
        self.refresh_emulators_ui();
        if self.bp.borrow().active {
            self.rebuild_bp_rows();
        }

        let new_updates: Vec<(GameId, String)> =
            results.iter().filter_map(|(_, r)| r.as_ref().ok()).flat_map(|s| s.new_updates.iter().cloned()).collect();
        self.notify_new_updates(&new_updates);
        self.refresh_updates();
        self.refresh_smart_ui();
        let added: usize = results.iter().filter_map(|(_, r)| r.as_ref().ok()).map(|s| s.added).sum();
        let total = self.lib.borrow().games.iter().filter(|g| !g.hidden).count();
        let status = trn!(total, "Terminée en {secs} s · {n} jeu", "Terminée en {secs} s · {n} jeux", secs = halcyon_i18n::decimal(elapsed.as_secs_f64(), 1));
        // Nouveaux jeux installés (pas au tout premier inventaire).
        if added > 0 && total > added {
            self.sound(halcyon_audio::Cue::Install);
        }
        let scan = self.tasks.borrow().find_kind("scan");
        if let Some(id) = scan {
            let detail = if added > 0 { trn!(added, "{status} · {n} nouveau", "{status} · {n} nouveaux", status = status) } else { status.clone() };
            self.tasks.borrow_mut().finish(id, true, detail, crate::tasks::Action::OpenPage("library"), now_unix());
        }
        self.publish_tasks();
        if let Some(app) = self.app() {
            let ui = app.global::<AppUi>();
            ui.set_scanning(false);
            ui.set_scan_status(status.into());
        }
        let local_errors: Vec<&String> = results
            .iter()
            .filter(|(s, r)| r.is_err() && !matches!(s, GameSource::Steam | GameSource::Epic))
            .filter_map(|(_, r)| r.as_ref().err())
            .collect();
        if let Some(e) = local_errors.first() {
            self.toast(tr!("Analyse incomplète : {e}", e = e), true);
        } else if added > 0 {
            self.toast(trn!(added, "{n} nouveau jeu ajouté", "{n} nouveaux jeux ajoutés"), false);
        }
        self.ensure_update_polling();
        self.start_online_jobs();
        if self.settings.borrow().sync_achievements_on_startup {
            self.start_ach_sync(None, false);
        }
    }

    // ------------------------------------------------------------------ visuels et métadonnées en ligne

    pub(crate) fn start_online_jobs(&self) {
        // Mode Performance : téléchargements reportés à la fin de la partie.
        if !self.net_allowed() || self.performance_active() {
            return;
        }
        let s = self.settings.borrow().clone();
        if !(s.online_artwork || s.online_metadata) || self.downloading.swap(true, Ordering::SeqCst) {
            return;
        }
        let mut failures = load_failures(&self.paths);
        let now = now_unix();
        failures.retain(|_, t| now - *t < RETRY_SECS);
        let mut art_jobs: Vec<(GameId, ArtworkKind, Vec<String>, PathBuf, String)> = Vec::new();
        let mut meta_jobs: Vec<(GameId, u64, String)> = Vec::new();
        // Sans clé : boutique Steam (jeux standalone et ajoutés à la main) et libretro-thumbnails
        // (jeux émulés). SteamGridDB, avec une clé, ne complète que ce qui manque encore.
        #[allow(clippy::type_complexity)]
        let mut store_jobs: Vec<(GameId, String, [bool; 3], PathBuf, String)> = Vec::new();
        #[allow(clippy::type_complexity)]
        let mut libretro_jobs: Vec<(GameId, String, Vec<String>, [bool; 2], PathBuf, String)> = Vec::new();
        // SteamGridDB : jeux standalone et ajoutés à la main sans visuel ni adresse connue.
        let sgdb_key = s.steamgriddb_api_key.trim().to_string();
        #[allow(clippy::type_complexity)]
        let mut sgdb_jobs: Vec<(GameId, String, [bool; 3], PathBuf, String)> = Vec::new();
        // Sources des extensions : jeux sans description que la boutique Steam ne couvre pas.
        let sources = if s.online_metadata { self.plugin_metadata_sources() } else { Vec::new() };
        #[allow(clippy::type_complexity)]
        let mut plugin_jobs: Vec<(GameId, usize, String, Option<String>, String)> = Vec::new();
        {
            let lib = self.lib.borrow();
            let dir = self.paths.artwork_dir();
            let mut games: Vec<&Game> = lib.games.iter().filter(|g| !g.hidden).collect();
            games.sort_by(|a, b| b.installed.cmp(&a.installed).then_with(|| a.sort_key.cmp(&b.sort_key)));
            for g in games {
                let stem = format!("{}-{}", g.source.as_str(), sanitize(&g.source_key));
                if s.online_artwork {
                    let a = &g.artwork;
                    for (kind, local, url, suffix) in [
                        (ArtworkKind::Cover, &a.cover, &a.cover_url, "cover"),
                        (ArtworkKind::Banner, &a.banner, &a.banner_url, "banner"),
                        (ArtworkKind::Logo, &a.logo, &a.logo_url, "logo"),
                    ] {
                        let missing = local.as_ref().is_none_or(|p| !p.is_file());
                        if let (true, Some(url)) = (missing, url) {
                            let key = format!("{stem}-{suffix}");
                            if !failures.contains_key(&key) {
                                art_jobs.push((g.id, kind, url_candidates(url), dir.join(&key), key));
                            }
                        }
                    }
                }
                // Visuel à chercher : absent du disque, sans adresse connue ou adresse en échec.
                let need = |local: &Option<PathBuf>, url: &Option<String>, suffix: &str| {
                    local.as_ref().is_none_or(|p| !p.is_file()) && (url.is_none() || failures.contains_key(&format!("{stem}-{suffix}")))
                };
                // Jeux sans visuels fournis par leur plateforme : autonomes, ajoutés à la main et
                // autres launchers (Ubisoft, EA, Xbox…), cherchés par leur nom.
                let by_name = matches!(g.source, GameSource::Standalone | GameSource::Manual) || GameSource::LAUNCHERS.contains(&g.source);
                if s.online_artwork && by_name && store_jobs.len() < STORE_PER_RUN {
                    let a = &g.artwork;
                    let want = [need(&a.cover, &a.cover_url, "cover"), need(&a.banner, &a.banner_url, "banner"), need(&a.logo, &a.logo_url, "logo")];
                    let key = format!("steamstore-{stem}");
                    if want.iter().any(|w| *w) && !failures.contains_key(&key) {
                        store_jobs.push((g.id, g.name.clone(), want, dir.join(&stem), key));
                    }
                }
                if s.online_artwork && g.source == GameSource::Emulated && libretro_jobs.len() < LIBRETRO_PER_RUN {
                    let system = g.platform.as_deref().and_then(|p| self.registry.borrow().platform(p).and_then(|d| d.libretro.clone()));
                    let a = &g.artwork;
                    let want = [need(&a.cover, &a.cover_url, "cover"), need(&a.banner, &a.banner_url, "banner")];
                    let key = format!("libretro-{stem}");
                    if let (Some(system), true, false) = (system, want.iter().any(|w| *w), failures.contains_key(&key)) {
                        let titles = halcyon_integrations::libretro::titles(g.executable.as_deref(), &g.name);
                        libretro_jobs.push((g.id, system, titles, want, dir.join(&stem), key));
                    }
                }
                if s.online_artwork
                    && !sgdb_key.is_empty()
                    && (by_name || g.source == GameSource::Minecraft)
                    && sgdb_jobs.len() < SGDB_PER_RUN
                {
                    let a = &g.artwork;
                    let want = [need(&a.cover, &a.cover_url, "cover"), need(&a.banner, &a.banner_url, "banner"), need(&a.logo, &a.logo_url, "logo")];
                    let key = format!("sgdb-{stem}");
                    if want.iter().any(|w| *w) && !failures.contains_key(&key) {
                        sgdb_jobs.push((g.id, g.name.clone(), want, dir.join(&stem), key));
                    }
                }
                if s.online_metadata
                    && g.source == GameSource::Steam
                    && g.description.is_none()
                    && meta_jobs.len() < METADATA_PER_RUN
                {
                    let key = format!("meta-{}", g.source_key);
                    if let (false, Ok(appid)) = (failures.contains_key(&key), g.source_key.parse::<u64>()) {
                        meta_jobs.push((g.id, appid, key));
                    }
                } else if g.description.is_none() && plugin_jobs.len() < METADATA_PER_RUN {
                    let appid = (g.source == GameSource::Steam).then(|| g.source_key.clone());
                    for (i, (m, src)) in sources.iter().enumerate() {
                        let key = format!("plugin-{}-{}-{}-{}", m.id, src.name, g.source.as_str(), sanitize(&g.source_key));
                        if !failures.contains_key(&key) {
                            plugin_jobs.push((g.id, i, g.name.clone(), appid.clone(), key));
                            break;
                        }
                    }
                }
            }
        }
        if art_jobs.is_empty() && meta_jobs.is_empty() && plugin_jobs.is_empty() && sgdb_jobs.is_empty() && store_jobs.is_empty() && libretro_jobs.is_empty() {
            self.downloading.store(false, Ordering::SeqCst);
            return;
        }
        log::info!(
            "en ligne : {} visuels, {} jeux dans la boutique Steam, {} sur libretro-thumbnails, {} sur SteamGridDB, {} fiches à récupérer",
            art_jobs.len(),
            store_jobs.len(),
            libretro_jobs.len(),
            sgdb_jobs.len(),
            meta_jobs.len()
        );
        let task = self.task_start("online", tr!("Visuels et fiches en ligne"));
        self.task_update(
            task,
            None,
            Some(&tr!(
                "Visuels à récupérer : {sgdb_jobs} · fiches : {plugin_jobs}",
                sgdb_jobs = art_jobs.len() + store_jobs.len() + libretro_jobs.len() + sgdb_jobs.len(),
                plugin_jobs = meta_jobs.len() + plugin_jobs.len()
            )),
        );
        let db = self.db.clone();
        let flag = self.downloading.clone();
        let paths = self.paths.clone();
        std::thread::Builder::new()
            .name("halcyon-online".into())
            .spawn(move || {
                let dl = Downloader::new();
                let mut network_errors = 0;
                let mut ok = 0;
                for (id, kind, urls, stem, fail_key) in art_jobs {
                    match dl.fetch_first(&urls, &stem) {
                        Ok(path) => {
                            network_errors = 0;
                            ok += 1;
                            if let Err(e) = db.lock().unwrap_or_else(|e| e.into_inner()).set_artwork(id, kind, Some(&path), false) {
                                log::error!("{e}");
                                continue;
                            }
                            post(move |c| c.on_artwork_downloaded(id, kind, path));
                        }
                        Err(FetchError::Network(e)) => {
                            network_errors += 1;
                            if network_errors >= 3 {
                                log::warn!("réseau indisponible : téléchargements interrompus ({e})");
                                break;
                            }
                        }
                        Err(FetchError::Unavailable(_)) => {
                            failures.insert(fail_key, now_unix());
                        }
                    }
                }
                // Visuels obtenus sans clé, par jeu (cover, bannière, logo) : SteamGridDB ne
                // cherche ensuite que ce qui manque encore.
                let mut got: HashMap<GameId, [bool; 3]> = HashMap::new();
                // Télécharge la première adresse qui répond et l'enregistre comme visuel du jeu.
                let save = |id: GameId, kind: ArtworkKind, urls: &[String], dest: &Path| -> Result<bool, FetchError> {
                    match dl.fetch_first(urls, dest) {
                        Ok(path) => {
                            if db.lock().unwrap_or_else(|e| e.into_inner()).set_artwork(id, kind, Some(&path), false).is_ok() {
                                post(move |c| c.on_artwork_downloaded(id, kind, path));
                            }
                            Ok(true)
                        }
                        Err(FetchError::Unavailable(_)) => Ok(false),
                        Err(e) => Err(e),
                    }
                };
                let named = |stem: &Path, suffix: &str| stem.with_file_name(format!("{}-{suffix}", stem.file_name().and_then(|n| n.to_str()).unwrap_or("jeu")));
                if network_errors < 3 {
                    'store: for (id, name, want, stem, key) in store_jobs {
                        std::thread::sleep(std::time::Duration::from_millis(1600));
                        match steam_web::store_search(&dl, &name) {
                            Ok(Some(appid)) => {
                                let urls = steam::cdn_artwork(appid);
                                let mut found = [false; 3];
                                for (i, (kind, suffix)) in [(ArtworkKind::Cover, "cover"), (ArtworkKind::Banner, "banner"), (ArtworkKind::Logo, "logo")].into_iter().enumerate() {
                                    if !want[i] {
                                        continue;
                                    }
                                    match save(id, kind, &url_candidates(&urls[i]), &named(&stem, suffix)) {
                                        Ok(true) => {
                                            found[i] = true;
                                            ok += 1;
                                        }
                                        Ok(false) => {}
                                        Err(e) => {
                                            log::warn!("boutique Steam interrompue : {e}");
                                            network_errors = 3;
                                            break 'store;
                                        }
                                    }
                                }
                                log::info!("boutique Steam : « {name} » → {appid}");
                                got.insert(id, found);
                                // Visuel absent de la boutique (souvent le logo) : nouvel essai
                                // après le délai habituel, pas à chaque démarrage.
                                if want.iter().zip(&found).any(|(w, f)| *w && !f) {
                                    failures.insert(key, now_unix());
                                }
                            }
                            Ok(None) | Err(FetchError::Unavailable(_)) => {
                                failures.insert(key, now_unix());
                            }
                            Err(FetchError::Network(e)) => {
                                log::warn!("boutique Steam interrompue : {e}");
                                network_errors = 3;
                                break;
                            }
                        }
                    }
                }
                if network_errors < 3 {
                    'libretro: for (id, system, titles, want, stem, key) in libretro_jobs {
                        use halcyon_integrations::libretro::{candidates, Thumb};
                        // Dépôts GitHub publics : rythme modéré.
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        let mut missing = false;
                        for (i, (kind, thumb, suffix)) in [(ArtworkKind::Cover, Thumb::Boxart, "cover"), (ArtworkKind::Banner, Thumb::Snap, "banner")].into_iter().enumerate() {
                            if !want[i] {
                                continue;
                            }
                            match save(id, kind, &candidates(&system, thumb, &titles), &named(&stem, suffix)) {
                                Ok(true) => ok += 1,
                                Ok(false) => missing = true,
                                Err(e) => {
                                    log::warn!("libretro-thumbnails interrompu : {e}");
                                    network_errors = 3;
                                    break 'libretro;
                                }
                            }
                        }
                        if missing {
                            failures.insert(key, now_unix());
                        }
                    }
                }
                if network_errors < 3 {
                    for (id, name, mut want, stem, key) in sgdb_jobs {
                        if let Some(done) = got.get(&id) {
                            for (w, d) in want.iter_mut().zip(done) {
                                *w &= !d;
                            }
                            if !want.iter().any(|w| *w) {
                                continue;
                            }
                        }
                        std::thread::sleep(std::time::Duration::from_millis(250));
                        match halcyon_integrations::steamgriddb::find_art(&dl, &sgdb_key, &name, want) {
                            Ok(Some(art)) => {
                                let mut got = 0;
                                for (kind, url, suffix) in [
                                    (ArtworkKind::Cover, &art.cover, "cover"),
                                    (ArtworkKind::Banner, &art.banner, "banner"),
                                    (ArtworkKind::Logo, &art.logo, "logo"),
                                ] {
                                    let Some(url) = url else { continue };
                                    let dest = stem.with_file_name(format!("{}-{suffix}", stem.file_name().and_then(|n| n.to_str()).unwrap_or("jeu")));
                                    match dl.fetch_first(&url_candidates(url), &dest) {
                                        Ok(path) => {
                                            got += 1;
                                            ok += 1;
                                            if db.lock().unwrap_or_else(|e| e.into_inner()).set_artwork(id, kind, Some(&path), false).is_ok() {
                                                post(move |c| c.on_artwork_downloaded(id, kind, path));
                                            }
                                        }
                                        Err(e) => log::warn!("SteamGridDB, « {name} » : {e}"),
                                    }
                                }
                                log::info!("SteamGridDB : « {name} » (jeu {}) : {got} visuel(s)", art.game_id);
                                if got == 0 {
                                    failures.insert(key, now_unix());
                                }
                            }
                            Ok(None) => {
                                log::info!("SteamGridDB : « {name} » introuvable");
                                failures.insert(key, now_unix());
                            }
                            Err(FetchError::Unavailable(e)) if *e == tr!("accès refusé (clé d'API invalide ou profil privé)") => {
                                log::warn!("SteamGridDB : clé d'API refusée ({e})");
                                post(|c| c.toast(tr!("SteamGridDB a refusé la clé d'API : vérifiez-la dans Paramètres › Comptes"), true));
                                break;
                            }
                            Err(FetchError::Unavailable(e)) => {
                                log::warn!("SteamGridDB, « {name} » : {e}");
                                failures.insert(key, now_unix());
                            }
                            Err(FetchError::Network(e)) => {
                                log::warn!("SteamGridDB interrompu : {e}");
                                network_errors = 3;
                                break;
                            }
                        }
                    }
                }
                if network_errors < 3 {
                    for (id, appid, key) in meta_jobs {
                        // La boutique Steam limite à ~200 requêtes / 5 min.
                        std::thread::sleep(std::time::Duration::from_millis(1600));
                        match steam_web::store_metadata(&dl, appid) {
                            Ok(Some(meta)) => {
                                let _ = db.lock().unwrap_or_else(|e| e.into_inner()).apply_online_metadata(id, &meta);
                                post(move |c| c.on_metadata_updated(id));
                            }
                            Ok(None) | Err(FetchError::Unavailable(_)) => {
                                failures.insert(key, now_unix());
                            }
                            Err(FetchError::Network(e)) => {
                                log::warn!("métadonnées interrompues : {e}");
                                break;
                            }
                        }
                    }
                }
                if network_errors < 3 {
                    for (id, index, name, appid, key) in plugin_jobs {
                        let (m, src) = &sources[index];
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        match halcyon_integrations::plugins::fetch_metadata(&dl, m, src, &name, appid.as_deref()) {
                            Ok(meta) if meta.description.is_some() || !meta.genres.is_empty() || meta.developer.is_some() => {
                                let _ = db.lock().unwrap_or_else(|e| e.into_inner()).apply_online_metadata(id, &meta);
                                post(move |c| c.on_metadata_updated(id));
                            }
                            Ok(_) => {
                                failures.insert(key, now_unix());
                            }
                            Err(e) => {
                                log::warn!("source « {} » ({}) : {e}", src.name, m.id);
                                failures.insert(key, now_unix());
                            }
                        }
                    }
                }
                save_failures(&paths, &failures);
                log::info!("{ok} visuels téléchargés");
                flag.store(false, Ordering::SeqCst);
                let interrupted = network_errors >= 3;
                post(move |c| {
                    let detail = if interrupted {
                        trn!(ok, "{n} visuel récupéré · réseau indisponible, reprise plus tard", "{n} visuels récupérés · réseau indisponible, reprise plus tard")
                    } else {
                        trn!(ok, "{n} visuel récupéré", "{n} visuels récupérés")
                    };
                    c.task_finish(task, !interrupted, detail, crate::tasks::Action::None);
                });
            })
            .ok();
    }

    fn on_artwork_downloaded(&self, id: GameId, kind: ArtworkKind, path: PathBuf) {
        if let Some(g) = self.lib.borrow_mut().get_mut(id) {
            match kind {
                ArtworkKind::Cover => g.artwork.cover = Some(path),
                ArtworkKind::Banner => g.artwork.banner = Some(path),
                ArtworkKind::Logo => g.artwork.logo = Some(path),
            }
        }
        self.images.invalidate(id);
        self.game_changed(id);
    }

    fn on_metadata_updated(&self, id: GameId) {
        let game = self.db().get_game(id);
        if let Ok(g) = game {
            self.lib.borrow_mut().upsert(g);
            if self.detail_id.get() == Some(id) {
                self.refresh_detail();
            }
            self.refresh_filters();
        }
    }

    // ------------------------------------------------------------------ paramètres

    /// Sauvegardes et mods de la fiche ouverte (analyse plus coûteuse : pas à chaque rafraîchissement).
    pub(crate) fn refresh_game_tools(&self) {
        let Some(id) = self.detail_id.get() else { return };
        let g = self.lib.borrow().get(id).cloned();
        if let Some(g) = g {
            self.refresh_saves_tab(&g);
            if g.source != GameSource::Emulated {
                self.refresh_mods_tab(&g);
            }
        }
    }

    pub(crate) fn save_settings(&self) {
        if self.no_save.get() {
            return;
        }
        if let Err(e) = self.settings.borrow().save(&self.paths.settings_path()) {
            log::error!("enregistrement des paramètres impossible : {e}");
            self.toast(tr!("Paramètres non enregistrés : {e}", e = e), true);
        }
    }

    pub(crate) fn refresh_settings_ui(&self) {
        let Some(app) = self.app() else { return };
        let s = self.settings.borrow();
        let found = self.found.borrow();
        let lib = self.lib.borrow();
        let info = |source: GameSource, name: &str, enabled: bool, over: &Option<PathBuf>| {
            let (ok, path) = found.get(&source).cloned().unwrap_or_default();
            SourceInfo {
                key: source.as_str().into(),
                name: name.into(),
                enabled,
                found: ok,
                detected: path.into(),
                path_override: over.as_ref().map(|p| p.display().to_string()).unwrap_or_default().into(),
                count: lib.games.iter().filter(|g| g.source == source && g.installed).count() as i32,
            }
        };
        let sui = app.global::<SettingsUi>();
        sui.set_steam(info(GameSource::Steam, "Steam", s.steam_enabled, &s.steam_path));
        sui.set_epic(info(GameSource::Epic, &tr!("Epic Games (launcher Epic, Heroic ou Legendary)"), s.epic_enabled, &s.epic_manifests_path));
        sui.set_gog(info(GameSource::Gog, &tr!("GOG (Heroic ou GOG Galaxy)"), s.gog_enabled, &s.gog_heroic_path));
        let launchers: Vec<SourceInfo> = GameSource::LAUNCHERS
            .into_iter()
            .filter(|l| halcyon_integrations::launchers::LauncherScanner::supported(*l))
            .map(|l| info(l, &l.label(), !s.disabled_launchers.iter().any(|d| d == l.as_str()), &None))
            .collect();
        sui.set_launchers(ModelRc::new(VecModel::from(launchers)));
        let folders: Vec<SharedString> = s.scan_folders.iter().map(|p| p.display().to_string().into()).collect();
        sui.set_folders(ModelRc::new(VecModel::from(folders)));
        sui.set_online_artwork(s.online_artwork);
        drop(s);
        self.lb_publish();
        let s = self.settings.borrow();
        sui.set_scan_on_startup(s.scan_on_startup);
        sui.set_minimize_on_launch(s.minimize_on_launch);
        sui.set_launch_screen(s.launch_screen);
        sui.set_break_reminder(s.break_reminder_hours.min(3) as i32);
        sui.set_free_games(s.free_games_notifications);
        sui.set_disk_alerts(s.disk_alerts);
        sui.set_discord_presence(s.discord_presence);
        sui.set_discord_client_id(s.discord_client_id.as_str().into());
        sui.set_animated_banner(s.animated_banner);
        sui.set_is_linux(cfg!(target_os = "linux"));
        sui.set_windows_runner(s.windows_runner.as_str().into());
        sui.set_steam_key(s.steam_api_key.as_str().into());
        sui.set_steam_id(s.steam_id.as_str().into());
        sui.set_steam_id_detected(self.local_steam_id().map(|i| i.to_string()).unwrap_or_default().into());
        sui.set_steam_local(self.steam_local().is_some());
        sui.set_ra_user(s.ra_user.as_str().into());
        sui.set_ra_key(s.ra_api_key.as_str().into());
        sui.set_online_metadata(s.online_metadata);
        sui.set_notifications(s.achievement_notifications);
        sui.set_polling(s.achievement_polling);
        sui.set_sync_on_startup(s.sync_achievements_on_startup);
        sui.set_check_updates(s.check_updates);
        sui.set_update_repo(s.update_repo.as_str().into());
        sui.set_start_big_picture(s.start_in_big_picture);
        sui.set_update_notifications(s.update_notifications);
        sui.set_nexus_key(s.nexus_api_key.as_str().into());
        sui.set_steamgriddb_key(s.steamgriddb_api_key.as_str().into());
        sui.set_offline_forced(s.offline_forced);
        sui.set_backups_dir(self.paths.data_dir.join("backups").to_string_lossy().into_owned().into());
        let thumbs = images::dir_size(&self.paths.thumbs_dir())
            + images::dir_size(&self.paths.artwork_dir())
            + images::dir_size(&self.paths.cache_dir.join("remote"));
        sui.set_cache_size(
            tr!("{thumbs} sur le disque · {u64} en mémoire", thumbs = util::format_size(thumbs), u64 = util::format_size(self.images.memory_usage() as u64))
                .into(),
        );
        sui.set_data_dir(self.paths.data_dir.display().to_string().into());
        sui.set_log_dir(self.paths.logs_dir.display().to_string().into());
        sui.set_version(env!("CARGO_PKG_VERSION").into());
    }

    fn set_source_enabled(&self, key: &str, enabled: bool) {
        {
            let mut s = self.settings.borrow_mut();
            match key {
                "steam" => s.steam_enabled = enabled,
                "epic" => s.epic_enabled = enabled,
                "gog" => s.gog_enabled = enabled,
                k if GameSource::LAUNCHERS.iter().any(|l| l.as_str() == k) => {
                    s.disabled_launchers.retain(|d| d != k);
                    if !enabled {
                        s.disabled_launchers.push(k.to_string());
                    }
                }
                _ => return,
            }
        }
        self.save_settings();
        self.refresh_settings_ui();
    }

    fn set_source_path(&self, key: &str, path: &str) {
        let path = Some(PathBuf::from(path.trim())).filter(|p| !p.as_os_str().is_empty());
        if let Some(p) = &path {
            if !p.is_dir() {
                self.toast(tr!("Dossier introuvable : {p}", p = p.display()), true);
                return;
            }
        }
        {
            let mut s = self.settings.borrow_mut();
            match key {
                "steam" => s.steam_path = path,
                "epic" => s.epic_manifests_path = path,
                "gog" => s.gog_heroic_path = path,
                _ => return,
            }
        }
        self.save_settings();
        self.detect_sources();
        self.refresh_settings_ui();
        let ok = self.found.borrow().get(&if key == "steam" { GameSource::Steam } else { GameSource::Epic }).is_some_and(|f| f.0);
        self.toast(
            if ok { tr!("Source trouvée — lancez une analyse pour importer les jeux") } else { tr!("Source toujours introuvable à cet emplacement") },
            !ok,
        );
    }

    fn add_folder(&self, path: &str) {
        let path = PathBuf::from(path.trim().trim_matches('"'));
        if path.as_os_str().is_empty() {
            return;
        }
        if !path.is_dir() {
            self.toast(tr!("Dossier introuvable : {path}", path = path.display()), true);
            return;
        }
        let key = util::path_key(&path);
        {
            let mut s = self.settings.borrow_mut();
            if s.scan_folders.iter().any(|p| util::path_key(p) == key) {
                drop(s);
                self.toast(tr!("Ce dossier est déjà surveillé"), true);
                return;
            }
            s.scan_folders.push(path);
        }
        self.save_settings();
        self.refresh_settings_ui();
        self.start_scan();
    }

    fn remove_folder(&self, index: usize) {
        {
            let mut s = self.settings.borrow_mut();
            if index >= s.scan_folders.len() {
                return;
            }
            s.scan_folders.remove(index);
        }
        self.save_settings();
        self.refresh_settings_ui();
        self.start_scan();
    }

    fn set_option(&self, name: &str, value: bool) {
        {
            let mut s = self.settings.borrow_mut();
            match name {
                "online_artwork" => s.online_artwork = value,
                "scan_on_startup" => s.scan_on_startup = value,
                "minimize_on_launch" => s.minimize_on_launch = value,
                "online_metadata" => s.online_metadata = value,
                "achievement_notifications" => s.achievement_notifications = value,
                "achievement_polling" => s.achievement_polling = value,
                "sync_achievements_on_startup" => s.sync_achievements_on_startup = value,
                "check_updates" => s.check_updates = value,
                "start_in_big_picture" => s.start_in_big_picture = value,
                "offline_forced" => s.offline_forced = value,
                "launch_screen" => s.launch_screen = value,
                "free_games_notifications" => s.free_games_notifications = value,
                "disk_alerts" => s.disk_alerts = value,
                "discord_presence" => s.discord_presence = value,
                "animated_banner" => s.animated_banner = value,
                _ => return,
            }
        }
        self.save_settings();
        self.refresh_settings_ui();
        if (name == "online_artwork" || name == "online_metadata") && value {
            self.start_online_jobs();
        }
        if name == "free_games_notifications" && value {
            self.check_free_games();
        }
        if name == "disk_alerts" && value {
            self.check_disks();
        }
        if name == "discord_presence" {
            self.update_presence();
        }
        if name == "animated_banner" {
            self.refresh_detail_media();
        }
        if name == "offline_forced" {
            let online = self.online.get();
            if let Some(app) = self.app() {
                app.global::<AppUi>().set_online(online && !value);
            }
            if !value && online {
                self.start_online_jobs();
            }
        }
    }

    fn set_text(&self, key: &str, value: &str) {
        let v = value.trim().to_string();
        {
            let mut s = self.settings.borrow_mut();
            match key {
                "steam_api_key" => s.steam_api_key = v.clone(),
                "steam_id" => {
                    if !v.is_empty() && v.parse::<u64>().is_err() {
                        drop(s);
                        self.toast(tr!("Le SteamID64 est un nombre (17 chiffres)"), true);
                        return;
                    }
                    s.steam_id = v.clone();
                }
                "ra_user" => s.ra_user = v.clone(),
                "ra_api_key" => s.ra_api_key = v.clone(),
                "windows_runner" => s.windows_runner = if v.is_empty() { "wine".into() } else { v.clone() },
                "update_repo" => s.update_repo = v.clone(),
                "nexus_api_key" => s.nexus_api_key = v.clone(),
                "steamgriddb_api_key" => s.steamgriddb_api_key = v.clone(),
                "discord_client_id" => {
                    if !v.chars().all(|c| c.is_ascii_digit()) {
                        drop(s);
                        self.toast(tr!("L'identifiant d'application Discord est un nombre"), true);
                        return;
                    }
                    s.discord_client_id = v.clone();
                }
                _ => return,
            }
        }
        self.save_settings();
        if key == "steamgriddb_api_key" {
            self.start_online_jobs();
        }
        self.refresh_settings_ui();
        self.refresh_ach_page();
        self.toast(tr!("Enregistré"), false);
    }

    fn clear_cache(&self) {
        let mut removed = 0u64;
        for dir in [self.paths.thumbs_dir(), self.paths.cache_dir.join("remote")] {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for e in entries.flatten() {
                    let len = e.metadata().map(|m| m.len()).unwrap_or(0);
                    if std::fs::remove_file(e.path()).is_ok() {
                        removed += len;
                    }
                }
            }
        }
        self.refresh_settings_ui();
        self.toast(tr!("{removed} libérés", removed = util::format_size(removed)), false);
    }

    pub(crate) fn save_window_state(&self) {
        if let Some(app) = self.app() {
            let w = app.window();
            if !self.bp.borrow().active {
                let scale = w.scale_factor();
                let size = w.size().to_logical(scale);
                let mut s = self.settings.borrow_mut();
                s.window_maximized = w.is_maximized();
                if !s.window_maximized && size.width > 0.0 {
                    s.window_width = size.width as u32;
                    s.window_height = size.height as u32;
                }
            }
        }
        self.save_settings();
    }

    pub(crate) fn local_steam_id(&self) -> Option<u64> {
        let root = steam::SteamScanner::new(self.settings.borrow().steam_path.clone()).root()?;
        steam::local_steam_id64(&root)
    }
}

fn sanitize(s: &str) -> String {
    let cleaned: String = s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    if cleaned.len() > 80 { format!("{:016x}", util::stable_hash(s)) } else { cleaned }
}

fn failures_path(paths: &AppPaths) -> PathBuf {
    paths.cache_dir.join("online-failures.json")
}

fn load_failures(paths: &AppPaths) -> HashMap<String, i64> {
    std::fs::read(failures_path(paths)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_failures(paths: &AppPaths, failures: &HashMap<String, i64>) {
    if let Ok(data) = serde_json::to_vec(failures) {
        let _ = std::fs::write(failures_path(paths), data);
    }
}

impl Controller {
    /// Actualités officielles d'un jeu Steam (Game Hub, 1.0.0) : en ligne uniquement, une
    /// requête par jeu et par heure au plus.
    pub(crate) fn refresh_game_news(&self, id: GameId) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<DetailUi>();
        let game = self.lib.borrow().get(id).map(|g| (g.source, g.source_key.clone()));
        let Some((GameSource::Steam, appid)) = game else {
            ui.set_news(ModelRc::default());
            ui.set_news_status("".into());
            return;
        };
        let now = halcyon_storage::now_unix();
        if let Some((at, items)) = self.news_cache.borrow().get(&id).cloned() {
            if now - at < 3600 {
                self.publish_news(id, &items, "");
                return;
            }
        }
        ui.set_news(ModelRc::default());
        if !self.net_allowed() {
            ui.set_news_status(tr!("Hors ligne : actualités indisponibles.").into());
            return;
        }
        ui.set_news_status(tr!("Chargement des actualités…").into());
        std::thread::Builder::new()
            .name("news".into())
            .spawn(move || {
                let dl = halcyon_integrations::artwork::Downloader::new();
                let r = halcyon_integrations::news::fetch_steam(&dl, &appid, 5);
                post(move |c| match r {
                    Ok(items) => {
                        c.news_cache.borrow_mut().insert(id, (now, items.clone()));
                        c.publish_news(id, &items, &if items.is_empty() { tr!("Aucune actualité publiée.") } else { String::new() });
                    }
                    Err(e) => {
                        log::info!("actualités indisponibles : {e}");
                        c.publish_news(id, &[], &tr!("Actualités indisponibles pour le moment."));
                    }
                });
            })
            .ok();
    }

    fn publish_news(&self, id: GameId, items: &[halcyon_integrations::news::NewsItem], status: &str) {
        if self.detail_id.get() != Some(id) {
            return;
        }
        let Some(app) = self.app() else { return };
        let ui = app.global::<DetailUi>();
        let rows: Vec<crate::NewsRow> = items
            .iter()
            .map(|n| crate::NewsRow {
                title: n.title.as_str().into(),
                url: n.url.as_str().into(),
                meta: [halcyon_core::util::format_date(n.date), n.source.clone()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ").into(),
                excerpt: n.excerpt.as_str().into(),
            })
            .collect();
        ui.set_news(ModelRc::new(VecModel::from(rows)));
        ui.set_news_status(status.into());
    }
}

/// Noms des étiquettes de la boutique Steam dans la langue de Halcyon : copie locale,
/// renouvelée au plus une fois par mois (liste publique, sans clé). Hors ligne : la copie.
fn steam_tag_names(cache: &Path, online: bool) -> HashMap<u64, String> {
    const MAX_AGE: i64 = 30 * 86_400;
    let saved: serde_json::Value = std::fs::read_to_string(cache).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let local: HashMap<u64, String> = saved
        .get("tags")
        .and_then(|t| t.as_object())
        .map(|m| m.iter().filter_map(|(k, v)| Some((k.parse().ok()?, v.as_str()?.to_string()))).collect())
        .unwrap_or_default();
    let fresh = saved.get("fetched").and_then(|v| v.as_i64()).is_some_and(|t| now_unix() - t < MAX_AGE);
    if !online || (fresh && !local.is_empty()) {
        return local;
    }
    let url = halcyon_integrations::steam_appinfo::tag_list_url(steam_web::steam_language());
    match Downloader::new().get_json(&url).map(|v| halcyon_integrations::steam_appinfo::parse_tag_list(&v)) {
        Ok(tags) if !tags.is_empty() => {
            let map: serde_json::Map<String, serde_json::Value> = tags.iter().map(|(k, v)| (k.to_string(), serde_json::Value::from(v.as_str()))).collect();
            let _ = std::fs::write(cache, serde_json::json!({ "fetched": now_unix(), "tags": map }).to_string());
            tags
        }
        Ok(_) => local,
        Err(e) => {
            log::warn!("étiquettes Steam non téléchargées : {e}");
            local
        }
    }
}
