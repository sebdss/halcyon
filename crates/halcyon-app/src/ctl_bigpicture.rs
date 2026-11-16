//! Contrôleur du mode console (Big Picture, refonte 1.0).
//!
//! Trois sections au sommet de l'écran (Jouer, Bibliothèque, Recherche), parcourues par la
//! croix ou les gâchettes, et des zones de focus verticales dans chacune :
//!
//! - **Jouer** : rangée de tuiles (jeux récents, favoris, installés, puis la bibliothèque),
//!   puis les actions du jeu sélectionné (Jouer, Favori, Options) ; le jeu remplit l'écran.
//! - **Bibliothèque** : filtres (Installés, Tous, Favoris, Récents, Émulation, collections),
//!   puis grille de grandes jaquettes ; les gâchettes changent de filtre.
//! - **Recherche** : clavier virtuel et résultats.
//!
//! Par-dessus : la **fiche** d'un jeu (actions et succès), le **menu d'options** d'un jeu et
//! le **centre de contrôle** (Start / Select). Toute action affichée fonctionne ; l'état est
//! dans Rust, l'interface ne fait que l'afficher (`BpUi`).

use crate::controller::Controller;
use crate::gamepad::PadAction;
use crate::models::{card_for, detail_for};
use crate::{AchievementRow, BpRow, BpUi, GameCard, KeyValue};
use halcyon_core::{Game, GameId, GameSource};
use halcyon_storage::now_unix;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::rc::Rc;

/// Jeux de la rangée de tuiles (avant la tuile « Bibliothèque »).
const STRIP_MAX: usize = 20;
/// Colonnes de la grille de la bibliothèque.
const GRID_COLS: usize = 6;
/// Rangées transmises à l'interface (fenêtre glissante).
const GRID_ROWS: usize = 3;
/// Succès visibles dans la fiche.
const ACH_WINDOW: usize = 5;
const KEYS: &[&str] = &[
    "A", "Z", "E", "R", "T", "Y", "U", "I", "O", "P", "Q", "S", "D", "F", "G", "H", "J", "K", "L", "M", "W", "X", "C", "V",
    "B", "N", "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "␣", "⌫",
];
const SECTIONS: usize = 3;
/// Entrées du centre de contrôle (voir `bp_menu_rows`).
const MENU_ITEMS: usize = 10;

/// Actions du menu d'options d'un jeu.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Opt {
    Play,
    Sheet,
    Favorite,
    Hide,
    Desktop,
    Close,
}

#[derive(Default)]
pub struct BigPicture {
    pub active: bool,
    /// 0 Jouer, 1 Bibliothèque, 2 Recherche.
    section: usize,
    /// Zone de focus : 0 barre des sections, 1 première zone (tuiles, filtres, clavier),
    /// 2 deuxième zone (actions, grille, résultats).
    zone: u8,
    strip: Vec<GameId>,
    strip_index: usize,
    button: usize,
    filters: Vec<(String, Vec<GameId>)>,
    filter: usize,
    grid_index: usize,
    /// Fiche ouverte (jeu) ; bouton actif 0 Jouer, 1 Favori, 2 Succès.
    sheet: Option<GameId>,
    sheet_button: usize,
    ach_index: usize,
    ach_count: usize,
    options: Option<(GameId, Vec<Opt>)>,
    option_index: usize,
    menu_open: bool,
    setting: usize,
    query: String,
    key: usize,
    results: Vec<GameId>,
    result_col: usize,
}

/// Première rangée affichée de la grille (fenêtre glissante autour de la sélection).
fn grid_first_row(index: usize, len: usize) -> usize {
    let last_row = len.saturating_sub(1) / GRID_COLS;
    (index / GRID_COLS).saturating_sub(1).min(last_row.saturating_sub(GRID_ROWS - 1))
}

fn ids(v: &[&Game]) -> Vec<GameId> {
    v.iter().map(|g| g.id).collect()
}

impl Controller {
    pub(crate) fn enter_big_picture(self: &Rc<Self>) {
        let Some(app) = self.app() else { return };
        {
            let mut bp = self.bp.borrow_mut();
            if bp.active {
                return;
            }
            *bp = BigPicture { active: true, zone: 1, ..Default::default() };
        }
        self.sound(halcyon_audio::Cue::Mode);
        self.rebuild_bp_rows();
        app.window().set_fullscreen(true);
        app.global::<BpUi>().set_active(true);
        app.global::<BpUi>().set_pad_connected(self.pad_connected.get());
        // Horloge mise à jour toutes les 20 s, uniquement en mode console.
        let timer = slint::Timer::default();
        let weak = Rc::downgrade(self);
        timer.start(slint::TimerMode::Repeated, std::time::Duration::from_secs(20), move || {
            if let Some(c) = weak.upgrade() {
                c.update_clock();
            }
        });
        *self.bp_clock.borrow_mut() = Some(timer);
        self.update_clock();
        self.refresh_bp();
    }

    pub(crate) fn exit_big_picture(&self) {
        let Some(app) = self.app() else { return };
        self.bp.borrow_mut().active = false;
        *self.bp_clock.borrow_mut() = None;
        app.global::<BpUi>().set_active(false);
        app.window().set_fullscreen(false);
    }

    fn update_clock(&self) {
        if let Some(app) = self.app() {
            let ui = app.global::<BpUi>();
            ui.set_clock(local_clock().into());
            ui.set_downloads(halcyon_core::transfer::global().active() as i32);
        }
    }

    /// Rangée de tuiles et filtres de la bibliothèque, depuis la bibliothèque en mémoire.
    pub(crate) fn rebuild_bp_rows(&self) {
        let lib = self.lib.borrow();
        let mut visible: Vec<&Game> = lib.games.iter().filter(|g| !g.hidden).collect();
        visible.sort_by(|a, b| a.sort_key.cmp(&b.sort_key));
        let mut recent: Vec<&Game> = visible.iter().copied().filter(|g| g.last_played.is_some()).collect();
        recent.sort_by_key(|g| std::cmp::Reverse(g.last_played));

        // Tuiles : récents, puis favoris, puis installés (sans doublon).
        let mut strip: Vec<GameId> = Vec::new();
        for g in recent.iter().chain(visible.iter().filter(|g| g.favorite)).chain(visible.iter().filter(|g| g.installed)) {
            if strip.len() >= STRIP_MAX {
                break;
            }
            if !strip.contains(&g.id) {
                strip.push(g.id);
            }
        }

        let mut filters: Vec<(String, Vec<GameId>)> = vec![
            ("Installés".into(), ids(&visible.iter().copied().filter(|g| g.installed).collect::<Vec<_>>())),
            ("Tous".into(), ids(&visible)),
            ("Favoris".into(), ids(&visible.iter().copied().filter(|g| g.favorite).collect::<Vec<_>>())),
            ("Récents".into(), ids(&recent)),
            ("Émulation".into(), ids(&visible.iter().copied().filter(|g| g.source == GameSource::Emulated).collect::<Vec<_>>())),
        ];
        drop(lib);
        for c in self.collections.borrow().iter() {
            if let Ok(members) = self.db().collection_members(c.id) {
                let lib = self.lib.borrow();
                let mut v: Vec<&Game> = members.iter().filter_map(|id| lib.get(*id)).filter(|g| !g.hidden).collect();
                v.sort_by(|a, b| a.sort_key.cmp(&b.sort_key));
                filters.push((c.name.clone(), ids(&v)));
            }
        }
        filters.retain(|(_, l)| !l.is_empty());
        let mut bp = self.bp.borrow_mut();
        bp.strip_index = bp.strip_index.min(strip.len());
        bp.strip = strip;
        bp.filter = bp.filter.min(filters.len().saturating_sub(1));
        bp.filters = filters;
        let len = bp.filters.get(bp.filter).map(|(_, l)| l.len()).unwrap_or(0);
        bp.grid_index = bp.grid_index.min(len.saturating_sub(1));
    }

    /// Jeu mis en avant (fond d'écran, informations).
    fn bp_focused(&self) -> Option<GameId> {
        let bp = self.bp.borrow();
        if let Some((id, _)) = &bp.options {
            return Some(*id);
        }
        if let Some(id) = bp.sheet {
            return Some(id);
        }
        match bp.section {
            0 => bp.strip.get(bp.strip_index).copied(),
            1 => bp.filters.get(bp.filter).and_then(|(_, l)| l.get(bp.grid_index)).copied(),
            _ if bp.zone == 2 => bp.results.get(bp.result_col).copied(),
            _ => None,
        }
    }

    fn bp_cards(&self, ids: &[GameId]) -> Vec<GameCard> {
        let lib = self.lib.borrow();
        let now = now_unix();
        ids.iter().filter_map(|id| lib.get(*id)).map(|g| card_for(g, &self.images, now)).collect()
    }

    fn bp_options_for(&self, id: GameId) -> Vec<Opt> {
        let installed = self.lib.borrow().get(id).is_some_and(|g| g.installed);
        let mut v = Vec::new();
        if installed {
            v.push(Opt::Play);
        }
        v.extend([Opt::Sheet, Opt::Favorite, Opt::Hide, Opt::Desktop, Opt::Close]);
        v
    }

    /// Pousse l'état de navigation vers l'interface.
    pub(crate) fn refresh_bp(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<BpUi>();
        let focused = self.bp_focused();
        let game = focused.and_then(|id| self.lib.borrow().get(id).cloned());
        let now = now_unix();
        {
            let bp = self.bp.borrow();
            ui.set_section(bp.section as i32);
            ui.set_zone(bp.zone as i32);
            ui.set_menu_open(bp.menu_open);
            ui.set_sheet_open(bp.sheet.is_some());
            ui.set_options_open(bp.options.is_some());
        }

        // Jeu mis en avant.
        ui.set_has_focused(game.is_some());
        if let Some(g) = &game {
            ui.set_focused(detail_for(g, &self.images, crate::models::Tracking::default(), now));
            ui.set_focused_meta(self.meta_line(g, now).into());
            let p = self.progress.borrow().get(&g.id).copied();
            ui.set_focused_ach(p.map(|p| format!("{} / {}", p.unlocked, p.total)).unwrap_or_default().into());
            ui.set_focused_ach_ratio(p.map(|p| p.percent() / 100.0).unwrap_or(-1.0));
            ui.set_focused_state(
                if g.installed {
                    g.size_bytes.map(|b| format!("Installé · {}", halcyon_core::util::format_size(b))).unwrap_or_else(|| "Installé".into())
                } else {
                    "Non installé".into()
                }
                .into(),
            );
            ui.set_focused_update(if g.update.state.pending() { "Mise à jour disponible".into() } else { SharedString::new() });
        }

        let (section, strip, strip_index, button) = {
            let bp = self.bp.borrow();
            (bp.section, bp.strip.clone(), bp.strip_index, bp.button)
        };
        if section == 0 {
            ui.set_strip(ModelRc::new(VecModel::from(self.bp_cards(&strip))));
            ui.set_strip_index(strip_index as i32);
            ui.set_button(button as i32);
        }
        if section == 1 {
            let (names, list, filter, index) = {
                let bp = self.bp.borrow();
                let names: Vec<SharedString> = bp.filters.iter().map(|(n, l)| format!("{n}  {}", l.len()).into()).collect();
                (names, bp.filters.get(bp.filter).map(|(_, l)| l.clone()).unwrap_or_default(), bp.filter, bp.grid_index)
            };
            let start = grid_first_row(index, list.len()) * GRID_COLS;
            let window: Vec<GameId> = list.iter().skip(start).take(GRID_COLS * GRID_ROWS).copied().collect();
            ui.set_filters(ModelRc::new(VecModel::from(names)));
            ui.set_filter(filter as i32);
            ui.set_grid(ModelRc::new(VecModel::from(self.bp_cards(&window))));
            ui.set_grid_index((index - start) as i32);
            ui.set_grid_position(if list.is_empty() { SharedString::new() } else { format!("{} / {}", index + 1, list.len()).into() });
        }
        if section == 2 {
            let (query, key, results, col, zone) = {
                let bp = self.bp.borrow();
                (bp.query.clone(), bp.key, bp.results.clone(), bp.result_col, bp.zone)
            };
            ui.set_query(query.as_str().into());
            ui.set_keys(ModelRc::new(VecModel::from(KEYS.iter().map(|k| SharedString::from(*k)).collect::<Vec<_>>())));
            ui.set_key_index(key as i32);
            let start = col.saturating_sub(2);
            let window: Vec<GameId> = results.iter().skip(start).take(8).copied().collect();
            ui.set_results(BpRow {
                title: if query.is_empty() { "Tapez un nom de jeu".into() } else { format!("{} résultat{}", results.len(), if results.len() > 1 { "s" } else { "" }).into() },
                items: ModelRc::new(VecModel::from(self.bp_cards(&window))),
                focus: if zone == 2 { (col - start) as i32 } else { -1 },
                count: results.len() as i32,
            });
        }

        // Fiche.
        let sheet = { self.bp.borrow().sheet };
        if let (Some(_), Some(g)) = (sheet, &game) {
            let (sb, ai) = {
                let bp = self.bp.borrow();
                (bp.sheet_button, bp.ach_index)
            };
            let list = self.db().load_achievements(g.id).unwrap_or_default();
            self.bp.borrow_mut().ach_count = list.len();
            let start = ai.saturating_sub(2).min(list.len().saturating_sub(ACH_WINDOW));
            let rows: Vec<AchievementRow> = list.iter().skip(start).take(ACH_WINDOW).map(|a| self.achievement_row(a, &g.name)).collect();
            ui.set_achievements(ModelRc::new(VecModel::from(rows)));
            ui.set_ach_count(list.len() as i32);
            ui.set_sheet_button(sb as i32);
            ui.set_ach_index(if sb == 2 { (ai - start) as i32 } else { -1 });
        }

        // Menu d'options.
        let opts = { self.bp.borrow().options.as_ref().map(|(_, o)| o.clone()) };
        if let (Some(opts), Some(g)) = (opts, &game) {
            let labels: Vec<SharedString> = opts
                .iter()
                .map(|o| match o {
                    Opt::Play => "Jouer".into(),
                    Opt::Sheet => "Ouvrir la fiche du jeu".into(),
                    Opt::Favorite if g.favorite => "Retirer des favoris".into(),
                    Opt::Favorite => "Ajouter aux favoris".into(),
                    Opt::Hide => "Masquer de la bibliothèque".into(),
                    Opt::Desktop => "Ouvrir sur le bureau".into(),
                    Opt::Close => "Fermer".into(),
                })
                .collect();
            ui.set_options(ModelRc::new(VecModel::from(labels)));
            ui.set_option_index(self.bp.borrow().option_index as i32);
        }

        // Centre de contrôle.
        if self.bp.borrow().menu_open {
            ui.set_settings(ModelRc::new(VecModel::from(self.bp_menu_rows())));
            ui.set_setting_index(self.bp.borrow().setting as i32);
        }
        ui.set_downloads(halcyon_core::transfer::global().active() as i32);
    }

    /// Centre de contrôle : libellé, état ; `ratio` = 1 pour une valeur réglable.
    fn bp_menu_rows(&self) -> Vec<KeyValue> {
        let a = self.settings.borrow().appearance.clone();
        let muted = self.settings.borrow().sound.muted;
        let theme = self.all_themes().into_iter().find(|t| t.id == a.theme).map(|t| t.name).unwrap_or_default();
        let t = halcyon_core::transfer::global();
        let kv = |label: &str, value: String| KeyValue { label: label.into(), value: value.into(), ratio: 1.0 };
        vec![
            kv("Mode Performance", if self.performance_active() { "Activé" } else { "Désactivé" }.into()),
            kv(
                "Téléchargements",
                if t.is_paused() {
                    "En pause".into()
                } else if t.active() > 0 {
                    format!("{} en cours", t.active())
                } else {
                    "Aucun".into()
                },
            ),
            kv("Son", if muted { "Coupé" } else { "Activé" }.into()),
            kv("Mouvement", ["Complet", "Réduit", "Aucun"][usize::from(a.motion().min(2))].into()),
            kv("Taille du texte", format!("{:.0} %", a.text_scale * 100.0)),
            kv("Thème", theme),
            kv("Analyser les sources", if self.scanning.get() { "En cours…".into() } else { String::new() }),
            kv("Synchroniser les succès", if self.ach_syncing.get() { "En cours…".into() } else { String::new() }),
            kv("Paramètres complets", "Sur le bureau".into()),
            kv("Quitter le mode console", String::new()),
        ]
    }

    fn bp_menu_activate(self: &Rc<Self>, index: usize, dir: i32) {
        let a = self.settings.borrow().appearance.clone();
        let cycle = |v: i32, n: i32| (v + dir).rem_euclid(n);
        match index {
            0 => {
                let on = !self.performance_active();
                self.set_performance(on);
            }
            1 => {
                let t = halcyon_core::transfer::global();
                self.set_downloads_paused(!t.is_paused());
            }
            2 => {
                let muted = self.settings.borrow().sound.muted;
                self.sound_toggle("muted", !muted);
            }
            3 => self.set_motion(cycle(i32::from(a.motion()), 3)),
            4 => {
                const SCALES: [f32; 4] = [0.9, 1.0, 1.15, 1.3];
                let i = SCALES.iter().position(|s| (s - a.text_scale).abs() < 0.04).unwrap_or(1) as i32;
                self.set_text_scale(SCALES[cycle(i, 4) as usize]);
            }
            5 => {
                let themes = self.all_themes();
                let i = themes.iter().position(|t| t.id == a.theme).unwrap_or(0) as i32;
                self.select_theme(cycle(i, themes.len().max(1) as i32) as usize);
            }
            6 => self.start_scan(),
            7 => self.start_ach_sync(None, true),
            8 => {
                self.exit_big_picture();
                if let Some(app) = self.app() {
                    app.global::<crate::Nav>().invoke_go(crate::Page::Settings);
                }
                return;
            }
            9 => return self.exit_big_picture(),
            _ => {}
        }
        self.refresh_bp();
    }

    fn bp_option_activate(self: &Rc<Self>) {
        let (id, opt) = {
            let mut bp = self.bp.borrow_mut();
            let Some((id, opts)) = bp.options.take() else { return };
            let opt = opts.get(bp.option_index).copied().unwrap_or(Opt::Close);
            if opt == Opt::Sheet {
                bp.sheet = Some(id);
                bp.sheet_button = 0;
                bp.ach_index = 0;
            }
            (id, opt)
        };
        match opt {
            Opt::Play => self.play(id),
            Opt::Favorite => {
                self.toggle_favorite(id);
                self.rebuild_bp_rows();
            }
            Opt::Hide => {
                self.toggle_hidden(id);
                self.rebuild_bp_rows();
            }
            Opt::Desktop => {
                self.exit_big_picture();
                self.open_game(id);
                return;
            }
            Opt::Sheet | Opt::Close => {}
        }
        self.refresh_bp();
    }

    fn bp_search_update(&self) {
        let query = self.bp.borrow().query.clone();
        let q = halcyon_core::LibraryQuery { text: query.clone(), ..Default::default() };
        let results: Vec<GameId> = {
            let lib = self.lib.borrow();
            if query.trim().is_empty() { vec![] } else { q.apply(&lib.games).into_iter().map(|i| lib.games[i].id).take(200).collect() }
        };
        let mut bp = self.bp.borrow_mut();
        bp.results = results;
        bp.result_col = 0;
    }

    /// Action de navigation (clavier, manette ou souris).
    pub(crate) fn bp_input(self: &Rc<Self>, action: &str) {
        if !self.bp.borrow().active {
            return;
        }
        if action == "exit" {
            return self.exit_big_picture();
        }
        let cue = match action {
            "up" | "down" | "left" | "right" => Some(halcyon_audio::Cue::GameSelect),
            "accept" => Some(halcyon_audio::Cue::Confirm),
            "back" => Some(halcyon_audio::Cue::Back),
            "search" | "settings" | "prev" | "next" => Some(halcyon_audio::Cue::PanelOpen),
            _ => None,
        };
        if let Some(cue) = cue {
            self.sound(cue);
        }

        // Centre de contrôle (Start / Select / Échap depuis l'accueil) : prioritaire.
        if action == "settings" {
            {
                let mut bp = self.bp.borrow_mut();
                bp.menu_open = !bp.menu_open;
                bp.options = None;
            }
            return self.refresh_bp();
        }
        if self.bp.borrow().menu_open {
            let cur = self.bp.borrow().setting;
            match action {
                // Grille de 2 × 5 tuiles.
                "left" => self.bp.borrow_mut().setting = cur.saturating_sub(1),
                "right" => self.bp.borrow_mut().setting = (cur + 1).min(MENU_ITEMS - 1),
                "up" => self.bp.borrow_mut().setting = cur.saturating_sub(5),
                "down" => self.bp.borrow_mut().setting = (cur + 5).min(MENU_ITEMS - 1),
                "back" => self.bp.borrow_mut().menu_open = false,
                "accept" => return self.bp_menu_activate(cur, 1),
                a if a.starts_with("menu:") => {
                    let i = a[5..].parse::<usize>().unwrap_or(0).min(MENU_ITEMS - 1);
                    self.bp.borrow_mut().setting = i;
                    return self.bp_menu_activate(i, 1);
                }
                _ => {}
            }
            return self.refresh_bp();
        }

        // Menu d'options d'un jeu.
        if self.bp.borrow().options.is_some() {
            let n = self.bp.borrow().options.as_ref().map(|(_, o)| o.len()).unwrap_or(1);
            match action {
                "up" | "left" => {
                    let mut bp = self.bp.borrow_mut();
                    bp.option_index = bp.option_index.saturating_sub(1);
                }
                "down" | "right" => {
                    let mut bp = self.bp.borrow_mut();
                    bp.option_index = (bp.option_index + 1).min(n - 1);
                }
                "back" => self.bp.borrow_mut().options = None,
                "accept" => return self.bp_option_activate(),
                a if a.starts_with("option:") => {
                    self.bp.borrow_mut().option_index = a[7..].parse::<usize>().unwrap_or(0).min(n - 1);
                    return self.bp_option_activate();
                }
                _ => {}
            }
            return self.refresh_bp();
        }

        // Fiche d'un jeu (emprunt relâché avant le bloc : il réemprunte en écriture).
        let sheet = self.bp.borrow().sheet;
        if let Some(id) = sheet {
            let mut play = false;
            let mut fav = false;
            {
                let mut bp = self.bp.borrow_mut();
                match action {
                    "left" => bp.sheet_button = bp.sheet_button.saturating_sub(1),
                    "right" => bp.sheet_button = (bp.sheet_button + 1).min(2),
                    "up" if bp.sheet_button == 2 => bp.ach_index = bp.ach_index.saturating_sub(1),
                    "down" if bp.sheet_button == 2 => bp.ach_index = (bp.ach_index + 1).min(bp.ach_count.saturating_sub(1)),
                    "down" => bp.sheet_button = 2,
                    "back" => bp.sheet = None,
                    "fav" => fav = true,
                    "accept" => match bp.sheet_button {
                        0 => play = true,
                        1 => fav = true,
                        _ => {}
                    },
                    a if a.starts_with("sheet:") => match a[6..].parse::<usize>().unwrap_or(9) {
                        0 => play = true,
                        1 => fav = true,
                        2 => bp.sheet_button = 2,
                        _ => {}
                    },
                    _ => {}
                }
            }
            if fav {
                self.toggle_favorite(id);
                self.rebuild_bp_rows();
            }
            self.refresh_bp();
            if play && self.lib.borrow().get(id).is_some_and(|g| g.installed) {
                self.play(id);
            }
            return;
        }

        let mut play: Option<GameId> = None;
        let mut favorite: Option<GameId> = None;
        let mut open_options: Option<GameId> = None;
        let mut search_changed = false;
        {
            let mut bp = self.bp.borrow_mut();
            let section = bp.section;
            // Sections : gâchettes, Tab (recherche), ou croix dans la barre du haut.
            match action {
                "prev" if !(section == 1 && bp.zone == 2) => {
                    bp.section = (section + SECTIONS - 1) % SECTIONS;
                    bp.zone = 1;
                }
                "next" if !(section == 1 && bp.zone == 2) => {
                    bp.section = (section + 1) % SECTIONS;
                    bp.zone = 1;
                }
                "search" => {
                    bp.section = 2;
                    bp.zone = 1;
                }
                a if a.starts_with("section:") => {
                    bp.section = a[8..].parse::<usize>().unwrap_or(0).min(SECTIONS - 1);
                    bp.zone = 1;
                }
                _ => {}
            }
            let section = bp.section;
            let zone = bp.zone;
            match (section, zone, action) {
                // Barre des sections.
                (_, 0, "left") => bp.section = (section + SECTIONS - 1) % SECTIONS,
                (_, 0, "right") => bp.section = (section + 1) % SECTIONS,
                (_, 0, "down" | "accept") => bp.zone = 1,
                (_, 1, "up") if section != 2 || bp.key < 10 => bp.zone = 0,

                // Jouer : tuiles.
                (0, 1, "left") => bp.strip_index = bp.strip_index.saturating_sub(1),
                (0, 1, "right") => bp.strip_index = (bp.strip_index + 1).min(bp.strip.len()),
                (0, 1, "accept" | "down") if bp.strip_index >= bp.strip.len() => {
                    // Tuile « Bibliothèque ».
                    if action == "accept" {
                        bp.section = 1;
                        bp.zone = 2;
                    }
                }
                (0, 1, "accept" | "down") => {
                    bp.zone = 2;
                    bp.button = 0;
                }
                (0, 1, "fav") => favorite = bp.strip.get(bp.strip_index).copied(),
                (0, 1, "context") => open_options = bp.strip.get(bp.strip_index).copied(),
                // Jouer : actions.
                (0, 2, "left") => bp.button = bp.button.saturating_sub(1),
                (0, 2, "right") => bp.button = (bp.button + 1).min(2),
                (0, 2, "up" | "back") => bp.zone = 1,
                (0, 2, "accept") => {
                    let id = bp.strip.get(bp.strip_index).copied();
                    match bp.button {
                        0 => play = id,
                        1 => favorite = id,
                        _ => open_options = id,
                    }
                }
                (0, 2, "fav") => favorite = bp.strip.get(bp.strip_index).copied(),

                // Bibliothèque : filtres.
                (1, 1, "left") => {
                    bp.filter = bp.filter.saturating_sub(1);
                    bp.grid_index = 0;
                }
                (1, 1, "right") => {
                    bp.filter = (bp.filter + 1).min(bp.filters.len().saturating_sub(1));
                    bp.grid_index = 0;
                }
                (1, 1, "down" | "accept") => bp.zone = 2,
                (1, 1, "back") => {
                    bp.section = 0;
                    bp.zone = 1;
                }
                // Bibliothèque : grille.
                (1, 2, a) => {
                    let len = bp.filters.get(bp.filter).map(|(_, l)| l.len()).unwrap_or(0);
                    let i = bp.grid_index;
                    let id = bp.filters.get(bp.filter).and_then(|(_, l)| l.get(i)).copied();
                    match a {
                        "left" if i % GRID_COLS > 0 => bp.grid_index = i - 1,
                        "right" if i % GRID_COLS < GRID_COLS - 1 && i + 1 < len => bp.grid_index = i + 1,
                        "up" if i >= GRID_COLS => bp.grid_index = i - GRID_COLS,
                        "up" | "back" => bp.zone = 1,
                        "down" if i + GRID_COLS < len => bp.grid_index = i + GRID_COLS,
                        "down" if (i / GRID_COLS + 1) * GRID_COLS < len => bp.grid_index = len - 1,
                        "prev" => {
                            bp.filter = bp.filter.saturating_sub(1);
                            bp.grid_index = 0;
                        }
                        "next" => {
                            bp.filter = (bp.filter + 1).min(bp.filters.len().saturating_sub(1));
                            bp.grid_index = 0;
                        }
                        "accept" => {
                            if let Some(id) = id {
                                bp.sheet = Some(id);
                                bp.sheet_button = 0;
                                bp.ach_index = 0;
                            }
                        }
                        "fav" => favorite = id,
                        "context" => open_options = id,
                        a if a.starts_with("grid:") => {
                            let first = grid_first_row(i, len) * GRID_COLS;
                            let k = first + a[5..].parse::<usize>().unwrap_or(0);
                            if k < len {
                                bp.grid_index = k;
                                bp.sheet = bp.filters.get(bp.filter).and_then(|(_, l)| l.get(k)).copied();
                                bp.sheet_button = 0;
                            }
                        }
                        _ => {}
                    }
                }

                // Recherche : clavier.
                (2, 1, a) => match a {
                    "left" => bp.key = bp.key.saturating_sub(1),
                    "right" => bp.key = (bp.key + 1).min(KEYS.len() - 1),
                    "up" => bp.key = bp.key.saturating_sub(10),
                    "down" => {
                        if bp.key + 10 < KEYS.len() {
                            bp.key += 10;
                        } else if !bp.results.is_empty() {
                            bp.zone = 2;
                        }
                    }
                    "accept" => {
                        match KEYS[bp.key] {
                            "␣" => bp.query.push(' '),
                            "⌫" => {
                                bp.query.pop();
                            }
                            k => bp.query.push_str(&k.to_lowercase()),
                        }
                        search_changed = true;
                    }
                    "back" if !bp.query.is_empty() => {
                        bp.query.pop();
                        search_changed = true;
                    }
                    "back" => {
                        bp.section = 0;
                        bp.zone = 1;
                    }
                    "backspace" => {
                        bp.query.pop();
                        search_changed = true;
                    }
                    a if a.starts_with("char:") => {
                        bp.query.push_str(&a[5..]);
                        search_changed = true;
                    }
                    _ => {}
                },
                // Recherche : résultats.
                (2, 2, a) => match a {
                    "up" | "back" => bp.zone = 1,
                    "left" => bp.result_col = bp.result_col.saturating_sub(1),
                    "right" => bp.result_col = (bp.result_col + 1).min(bp.results.len().saturating_sub(1)),
                    "accept" => {
                        bp.sheet = bp.results.get(bp.result_col).copied();
                        bp.sheet_button = 0;
                        bp.ach_index = 0;
                    }
                    "fav" => favorite = bp.results.get(bp.result_col).copied(),
                    "context" => open_options = bp.results.get(bp.result_col).copied(),
                    a if a.starts_with("char:") => {
                        bp.query.push_str(&a[5..]);
                        bp.zone = 1;
                        search_changed = true;
                    }
                    "backspace" => {
                        bp.query.pop();
                        search_changed = true;
                    }
                    _ => {}
                },
                _ => {}
            }
            // Souris : tuile ou résultat cliqué.
            if let Some(i) = action.strip_prefix("tile:").and_then(|v| v.parse::<usize>().ok()) {
                bp.section = 0;
                if bp.strip_index == i && bp.zone == 1 {
                    if i >= bp.strip.len() {
                        bp.section = 1;
                        bp.zone = 2;
                    } else {
                        bp.zone = 2;
                        bp.button = 0;
                    }
                } else {
                    bp.strip_index = i.min(bp.strip.len());
                    bp.zone = 1;
                }
            }
            if let Some(i) = action.strip_prefix("button:").and_then(|v| v.parse::<usize>().ok()) {
                bp.zone = 2;
                bp.button = i.min(2);
                let id = bp.strip.get(bp.strip_index).copied();
                match i {
                    0 => play = id,
                    1 => favorite = id,
                    _ => open_options = id,
                }
            }
            if let Some(i) = action.strip_prefix("filter:").and_then(|v| v.parse::<usize>().ok()) {
                bp.filter = i.min(bp.filters.len().saturating_sub(1));
                bp.grid_index = 0;
                bp.zone = 1;
            }
        }
        if search_changed {
            self.bp_search_update();
        }
        if let Some(id) = favorite {
            self.toggle_favorite(id);
            self.rebuild_bp_rows();
        }
        if let Some(id) = open_options {
            let opts = self.bp_options_for(id);
            let mut bp = self.bp.borrow_mut();
            bp.options = Some((id, opts));
            bp.option_index = 0;
        }
        self.refresh_bp();
        if let Some(id) = play {
            if self.lib.borrow().get(id).is_some_and(|g| g.installed) {
                self.play(id);
            } else {
                self.sound(halcyon_audio::Cue::Error);
            }
        }
    }

    /// Événement manette : actif en mode console, ou pour y entrer (bouton Start / Guide).
    pub(crate) fn on_pad(self: &Rc<Self>, action: PadAction) {
        // Jamais pendant une partie lancée par Halcyon, ni quand la fenêtre n'a pas le focus.
        if self.active_session.get().is_some() || !self.window_focused() {
            return;
        }
        let active = self.bp.borrow().active;
        match (active, action) {
            (false, PadAction::Menu) => self.enter_big_picture(),
            (true, PadAction::Menu) => self.bp_input("settings"),
            (true, PadAction::Context) => self.bp_input("context"),
            (true, PadAction::Prev) => self.bp_input("prev"),
            (true, PadAction::Next) => self.bp_input("next"),
            (true, a) => {
                if let Some(input) = a.as_input() {
                    self.bp_input(input);
                }
            }
            (false, a) => self.desktop_pad(a),
        }
    }

    fn window_focused(&self) -> bool {
        use slint::winit_030::WinitWindowAccessor;
        self.app().and_then(|app| app.window().with_winit_window(|w| w.has_focus())).unwrap_or(false)
    }
}

/// Heure locale « HH:MM » (fuseau du système, sans dépendance supplémentaire).
fn local_clock() -> String {
    let offset = local_offset_secs();
    let s = (now_unix() + offset).rem_euclid(86_400);
    format!("{:02}:{:02}", s / 3600, s / 60 % 60)
}

#[cfg(unix)]
pub(crate) fn local_offset_secs() -> i64 {
    // `date +%z` évite une dépendance au fuseau horaire ; lu une seule fois.
    use std::sync::OnceLock;
    static OFFSET: OnceLock<i64> = OnceLock::new();
    *OFFSET.get_or_init(|| {
        std::process::Command::new("date")
            .arg("+%z")
            .output()
            .ok()
            .and_then(|o| parse_offset(String::from_utf8_lossy(&o.stdout).trim()))
            .unwrap_or(0)
    })
}

#[cfg(windows)]
pub(crate) fn local_offset_secs() -> i64 {
    use std::os::windows::process::CommandExt;
    use std::sync::OnceLock;
    static OFFSET: OnceLock<i64> = OnceLock::new();
    *OFFSET.get_or_init(|| {
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", "[System.TimeZoneInfo]::Local.GetUtcOffset([DateTime]::Now).TotalSeconds"])
            .creation_flags(0x0800_0000)
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<f64>().ok())
            .map(|v| v as i64)
            .unwrap_or(0)
    })
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn local_offset_secs() -> i64 {
    0
}

/// Calcule le décalage horaire en arrière-plan au démarrage (évite d'attendre à l'ouverture de Big Picture).
pub fn prefetch_clock() {
    std::thread::spawn(|| {
        local_offset_secs();
    });
}

fn parse_offset(s: &str) -> Option<i64> {
    if s.len() != 5 {
        return None;
    }
    let sign = if s.starts_with('-') { -1 } else { 1 };
    let h: i64 = s[1..3].parse().ok()?;
    let m: i64 = s[3..5].parse().ok()?;
    Some(sign * (h * 3600 + m * 60))
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_utc_offsets() {
        assert_eq!(super::parse_offset("+0100"), Some(3600));
        assert_eq!(super::parse_offset("-0530"), Some(-19_800));
        assert_eq!(super::parse_offset("xx"), None);
    }
}
