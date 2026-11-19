//! Collections manuelles (1.0.0-alpha.6) : sélection multiple dans la bibliothèque, choix
//! de collection, création guidée par la recherche, suggestions de séries et page de
//! collection (nom, couleur, logo, bannière, ordre des jeux).
//!
//! Une collection mélange librement les sources. Rien n'est créé sans action explicite :
//! une suggestion refusée est mémorisée (`Settings::series_dismissed`) et jamais reproposée.
//! Les collections reprises de Steam (« Steam · … ») suivent Steam : leur contenu n'est pas
//! modifiable ici (seuls leur style et leur place changent).

use crate::controller::Controller;
use crate::images::{ImageKey, Kind};
use crate::models::card_for;
use crate::{CollGame, CollRow, CollUi, LibraryUi, SeriesRow};
use halcyon_core::collections::{self as coll, SeriesSuggestion};
use halcyon_core::{Collection, GameId, LibraryQuery};
use halcyon_storage::{now_unix, ActivityKind};
use slint::{Color, ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Correspondances affichées dans la création guidée.
const CREATE_MAX: usize = 200;

/// État des fenêtres et de la page des collections.
#[derive(Default)]
pub struct CollState {
    /// Jeux visés par le choix de collection en cours.
    targets: Vec<GameId>,
    /// Création guidée : correspondances (jeu, coché) et décochages conservés d'une recherche
    /// à l'autre.
    matches: Vec<(GameId, bool)>,
    unchecked: HashSet<GameId>,
    last_query: String,
    /// Page : collection affichée et ordre de ses jeux.
    current: Option<i64>,
    order: Vec<GameId>,
}

pub fn to_color(hex: &str) -> Color {
    let (r, g, b) = coll::parse_color(hex).unwrap_or((0x3d, 0x7f, 0xc4));
    Color::from_rgb_u8(r, g, b)
}

/// Couleur d'une collection : choisie, sinon dérivée de son nom.
pub fn collection_color(c: &Collection) -> Color {
    to_color(c.color.as_deref().filter(|h| coll::parse_color(h).is_some()).unwrap_or_else(|| coll::default_color(&c.name)))
}

impl Controller {
    pub(crate) fn wire_collections(self: &Rc<Self>, app: &crate::AppWindow) {
        let weak = Rc::downgrade(self);
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
        let ui = app.global::<CollUi>();
        ui.on_toggle_mark(with!(|c, id, range| c.toggle_mark(GameId::from(id), range)));
        ui.on_clear_marks(with!(|c| c.clear_marks()));
        ui.on_collect(with!(|c, id| c.open_collection_picker(GameId::from(id))));
        ui.on_picker_add(with!(|c, id| c.picker_add(i64::from(id))));
        ui.on_picker_create(with!(|c, name| c.picker_create(&name)));
        ui.on_open_create(with!(|c, prefill| c.open_create_collection(&prefill)));
        ui.on_create_search(with!(|c| c.create_search()));
        ui.on_create_toggle(with!(|c, i| c.create_toggle(i.max(0) as usize)));
        ui.on_create_all(with!(|c, on| c.create_all(on)));
        ui.on_create_confirm(with!(|c| c.create_confirm()));
        ui.on_select(with!(|c, id| c.select_collection(i64::from(id))));
        ui.on_select_delta(with!(|c, d| c.select_collection_delta(d)));
        ui.on_rename(with!(|c, name| c.rename_current_collection(&name)));
        ui.on_set_color(with!(|c, hex| c.set_collection_color(&hex)));
        ui.on_set_image(with!(|c, kind, path| c.set_collection_image(&kind, &path)));
        ui.on_move_collection(with!(|c, d| c.move_collection(d)));
        ui.on_delete_current(with!(|c| c.delete_current_collection()));
        ui.on_move_game(with!(|c, i, d| c.move_collection_game(i.max(0) as usize, d)));
        ui.on_remove_game(with!(|c, i| c.remove_collection_game(i.max(0) as usize)));
        ui.on_accept_suggestion(with!(|c, key| c.accept_suggestion(&key)));
        ui.on_dismiss_suggestion(with!(|c, key| c.dismiss_suggestion(&key)));
        ui.set_palette(ModelRc::new(VecModel::from(coll::PALETTE.iter().map(|h| to_color(h)).collect::<Vec<_>>())));
        ui.set_palette_hex(ModelRc::new(VecModel::from(coll::PALETTE.iter().map(|h| SharedString::from(*h)).collect::<Vec<_>>())));
    }

    /// Collections, filtres et vues qui en dépendent, après une modification.
    pub(crate) fn collections_changed(&self) {
        self.reload_collections();
        self.refresh_filters();
        self.refresh_library();
        self.refresh_detail();
        if self.bp.borrow().active {
            self.rebuild_bp_rows();
            self.refresh_bp();
        }
        if self.app().is_some_and(|a| a.global::<crate::Nav>().get_page() == crate::Page::Collections) {
            self.refresh_collections_page();
        }
    }

    // ------------------------------------------------------------------ sélection multiple

    /// Coche ou décoche un jeu ; `range` (Maj) coche tous les jeux affichés entre le dernier
    /// jeu coché et celui-ci.
    pub(crate) fn toggle_mark(&self, id: GameId, range: bool) {
        {
            let mut lib = self.lib.borrow_mut();
            let anchor = lib.mark_anchor.and_then(|a| lib.pos_of.get(&a).copied());
            match (range, anchor, lib.pos_of.get(&id).copied()) {
                (true, Some(a), Some(b)) => {
                    let (lo, hi) = (a.min(b), a.max(b));
                    let ids: Vec<GameId> = lib.filtered[lo..=hi].iter().map(|&i| lib.games[i].id).collect();
                    lib.marked.extend(ids);
                }
                _ => {
                    if !lib.marked.remove(&id) {
                        lib.marked.insert(id);
                    }
                }
            }
            lib.mark_anchor = Some(id);
        }
        self.sound(halcyon_audio::Cue::Setting);
        self.marks_changed();
    }

    pub(crate) fn clear_marks(&self) {
        {
            let mut lib = self.lib.borrow_mut();
            lib.marked.clear();
            lib.mark_anchor = None;
        }
        self.marks_changed();
    }

    /// Coche ou décoche le jeu sélectionné (bouton de la manette, Ctrl+Espace).
    pub(crate) fn toggle_mark_selected(&self) {
        if let Some(id) = self.lib_selected.get().filter(|_| self.selected_pos().is_some()) {
            self.toggle_mark(id, false);
        }
    }

    fn marks_changed(&self) {
        let n = self.lib.borrow().marked.len();
        if let Some(app) = self.app() {
            app.global::<CollUi>().set_marked_count(n as i32);
        }
        self.grid.reset();
        self.list.reset();
    }

    // ------------------------------------------------------------------ choix de collection

    /// Collections modifiables (celles de Steam suivent Steam).
    fn editable_collections(&self) -> Vec<Collection> {
        self.collections.borrow().iter().filter(|c| !coll::is_synced(&c.name)).cloned().collect()
    }

    fn coll_rows(&self, list: &[Collection]) -> Vec<CollRow> {
        list.iter()
            .map(|c| CollRow {
                id: c.id as i32,
                name: c.name.as_str().into(),
                count: c.count as i32,
                color: collection_color(c),
                synced: coll::is_synced(&c.name),
            })
            .collect()
    }

    /// « Ajouter à une collection… » : jeux cochés (si `id` en fait partie ou vaut -1),
    /// sinon le seul jeu `id`.
    pub(crate) fn open_collection_picker(&self, id: GameId) {
        let Some(app) = self.app() else { return };
        let marked: Vec<GameId> = {
            let lib = self.lib.borrow();
            let mut v: Vec<GameId> = lib.marked.iter().copied().collect();
            v.sort_by_key(|g| lib.pos_of.get(g).copied().unwrap_or(usize::MAX));
            v
        };
        let targets = if !marked.is_empty() && (id < 0 || marked.contains(&id)) {
            marked
        } else if id >= 0 {
            vec![id]
        } else {
            return;
        };
        let title = if targets.len() == 1 {
            self.lib.borrow().get(targets[0]).map(|g| g.name.clone()).unwrap_or_default()
        } else {
            trn!(targets.len(), "{n} jeu sélectionné", "{n} jeux sélectionnés")
        };
        self.coll.borrow_mut().targets = targets;
        let ui = app.global::<CollUi>();
        ui.set_collections(ModelRc::new(VecModel::from(self.coll_rows(&self.editable_collections()))));
        ui.set_picker_title(title.into());
        ui.set_picker_new(SharedString::new());
        ui.set_picker_index(0);
        ui.set_picker_open(true);
    }

    fn picker_add(&self, collection: i64) {
        let targets = std::mem::take(&mut self.coll.borrow_mut().targets);
        let Some(c) = self.collections.borrow().iter().find(|c| c.id == collection).cloned() else { return };
        if targets.is_empty() || coll::is_synced(&c.name) {
            return;
        }
        let added = self.db().add_to_collection(collection, &targets);
        self.finish_picker(&c.name, added.map(|n| (n, targets.len())));
    }

    fn picker_create(&self, name: &str) {
        let name = name.trim().to_string();
        let targets = std::mem::take(&mut self.coll.borrow_mut().targets);
        if name.is_empty() || targets.is_empty() {
            return;
        }
        if coll::is_synced(&name) {
            self.toast(tr!("Ce nom est réservé aux collections reprises de Steam."), true);
            return;
        }
        let result = self.db().create_collection_with(&name, &targets).map(|_| (targets.len(), targets.len()));
        self.finish_picker(&name, result);
    }

    fn finish_picker(&self, name: &str, result: halcyon_storage::Result<(usize, usize)>) {
        let Some(app) = self.app() else { return };
        match result {
            Ok((added, asked)) => {
                app.global::<CollUi>().set_picker_open(false);
                self.sound(halcyon_audio::Cue::Collection);
                let msg = if added == asked {
                    trn!(added, "{n} jeu ajouté à « {name} »", "{n} jeux ajoutés à « {name} »", name = name)
                } else {
                    tr!("{added} ajoutés à « {name} » ({already} y étaient déjà)", added = added, name = name, already = asked - added)
                };
                let _ = self.db().log_activity(
                    ActivityKind::Settings,
                    None,
                    &trn_later!(added, "{n} jeu ajouté à la collection « {name} »", "{n} jeux ajoutés à la collection « {name} »", name = name),
                    "",
                );
                self.toast(msg, false);
                self.clear_marks();
                self.collections_changed();
            }
            Err(e) => self.toast(tr!("Erreur : {e}", e = e), true),
        }
    }

    // ------------------------------------------------------------------ création guidée

    pub(crate) fn open_create_collection(&self, prefill: &str) {
        let Some(app) = self.app() else { return };
        {
            let mut st = self.coll.borrow_mut();
            st.unchecked.clear();
            st.matches.clear();
            st.last_query = prefill.trim().to_string();
        }
        let ui = app.global::<CollUi>();
        ui.set_create_query(prefill.trim().into());
        ui.set_create_name(title_case(prefill.trim()).into());
        ui.set_create_index(0);
        ui.set_create_open(true);
        self.create_search();
    }

    /// Recherche floue de la bibliothèque (fautes, sigles, filtres `source:`…) ; les jeux
    /// trouvés sont cochés, sauf ceux que l'on a décochés.
    fn create_search(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<CollUi>();
        let query = ui.get_create_query().to_string();
        // Le nom suit la recherche tant qu'il n'a pas été modifié à la main.
        {
            let mut st = self.coll.borrow_mut();
            let name = ui.get_create_name().to_string();
            if name.trim().is_empty() || name.trim() == st.last_query || name.trim() == title_case(&st.last_query) {
                ui.set_create_name(title_case(query.trim()).into());
            }
            st.last_query = query.trim().to_string();
        }
        let found: Vec<GameId> = if query.trim().is_empty() {
            Vec::new()
        } else {
            let lib = self.lib.borrow();
            let q = LibraryQuery { text: query.clone(), include_hidden: true, ..Default::default() };
            q.apply_with(&lib.games, &lib.ctx).into_iter().map(|i| lib.games[i].id).collect()
        };
        {
            let mut st = self.coll.borrow_mut();
            let unchecked = st.unchecked.clone();
            st.matches = found.iter().take(CREATE_MAX).map(|id| (*id, !unchecked.contains(id))).collect();
        }
        ui.set_create_more(found.len().saturating_sub(CREATE_MAX) as i32);
        ui.set_create_index(0);
        self.publish_create_matches();
    }

    fn publish_create_matches(&self) {
        let Some(app) = self.app() else { return };
        let now = now_unix();
        let (rows, checked) = {
            let st = self.coll.borrow();
            let lib = self.lib.borrow();
            let rows: Vec<CollGame> = st
                .matches
                .iter()
                .filter_map(|(id, on)| lib.get(*id).map(|g| (g, *on)))
                .map(|(g, on)| CollGame {
                    id: g.id as i32,
                    title: g.name.as_str().into(),
                    detail: game_detail(g).into(),
                    checked: on,
                    card: card_for(g, &self.images, now),
                })
                .collect();
            (rows, st.matches.iter().filter(|m| m.1).count())
        };
        let ui = app.global::<CollUi>();
        ui.set_create_matches(ModelRc::new(VecModel::from(rows)));
        ui.set_create_checked(checked as i32);
    }

    fn create_toggle(&self, index: usize) {
        {
            let mut st = self.coll.borrow_mut();
            let Some((id, on)) = st.matches.get_mut(index).map(|m| (m.0, &mut m.1)) else { return };
            *on = !*on;
            let now_on = *on;
            if now_on {
                st.unchecked.remove(&id);
            } else {
                st.unchecked.insert(id);
            }
        }
        // Mise à jour d'une seule ligne : la liste ne saute pas.
        let Some(app) = self.app() else { return };
        let ui = app.global::<CollUi>();
        let model = ui.get_create_matches();
        if let Some(mut row) = model.row_data(index) {
            row.checked = !row.checked;
            model.set_row_data(index, row);
        }
        ui.set_create_checked(self.coll.borrow().matches.iter().filter(|m| m.1).count() as i32);
    }

    fn create_all(&self, on: bool) {
        {
            let mut st = self.coll.borrow_mut();
            let ids: Vec<GameId> = st.matches.iter().map(|m| m.0).collect();
            for m in st.matches.iter_mut() {
                m.1 = on;
            }
            for id in ids {
                if on {
                    st.unchecked.remove(&id);
                } else {
                    st.unchecked.insert(id);
                }
            }
        }
        self.publish_create_matches();
    }

    fn create_confirm(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<CollUi>();
        let name = ui.get_create_name().trim().to_string();
        let ids: Vec<GameId> = self.coll.borrow().matches.iter().filter(|m| m.1).map(|m| m.0).collect();
        if name.is_empty() {
            self.toast(tr!("Donnez un nom à la collection."), true);
            return;
        }
        if coll::is_synced(&name) {
            self.toast(tr!("Ce nom est réservé aux collections reprises de Steam."), true);
            return;
        }
        let result = self.db().create_collection_with(&name, &ids);
        match result {
            Ok(id) => {
                ui.set_create_open(false);
                self.sound(halcyon_audio::Cue::Collection);
                let _ = self.db().log_activity(
                    ActivityKind::Settings,
                    None,
                    &trn_later!(ids.len(), "Collection « {name} » créée ({n} jeu)", "Collection « {name} » créée ({n} jeux)", name = name),
                    "",
                );
                self.toast(trn!(ids.len(), "Collection « {name} » créée ({n} jeu)", "Collection « {name} » créée ({n} jeux)", name = name), false);
                self.coll.borrow_mut().current = Some(id);
                self.collections_changed();
            }
            Err(e) => self.toast(tr!("Erreur : {e}", e = e), true),
        }
    }

    // ------------------------------------------------------------------ suggestions de séries

    fn series_suggestions(&self) -> Vec<SeriesSuggestion> {
        let grouped = self.db().games_in_collections().unwrap_or_default();
        let names: Vec<String> = self.collections.borrow().iter().map(|c| c.name.clone()).collect();
        let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let dismissed = self.settings.borrow().series_dismissed.clone();
        let lib = self.lib.borrow();
        let titles: Vec<(GameId, &str)> = lib.games.iter().filter(|g| !g.hidden).map(|g| (g.id, g.name.as_str())).collect();
        coll::suggest_series(&titles, &name_refs, &dismissed, &grouped)
    }

    fn accept_suggestion(&self, key: &str) {
        let Some(s) = self.series_suggestions().into_iter().find(|s| s.key == key) else { return };
        let result = self.db().create_collection_with(&s.name, &s.games);
        match result {
            Ok(id) => {
                self.sound(halcyon_audio::Cue::Collection);
                let _ = self.db().log_activity(
                    ActivityKind::Settings,
                    None,
                    &trn_later!(s.games.len(), "Collection « {name} » créée ({n} jeu)", "Collection « {name} » créée ({n} jeux)", name = s.name),
                    "",
                );
                self.toast(trn!(s.games.len(), "Collection « {name} » créée ({n} jeu)", "Collection « {name} » créée ({n} jeux)", name = s.name), false);
                self.coll.borrow_mut().current = Some(id);
                self.collections_changed();
            }
            Err(e) => self.toast(tr!("Erreur : {e}", e = e), true),
        }
    }

    fn dismiss_suggestion(&self, key: &str) {
        {
            let mut s = self.settings.borrow_mut();
            if !s.series_dismissed.iter().any(|k| k == key) {
                s.series_dismissed.push(key.to_string());
            }
        }
        self.save_settings();
        self.refresh_collections_page();
    }

    // ------------------------------------------------------------------ page Collections

    /// Ouvre la page d'une collection (ou la première).
    pub(crate) fn open_collections_page(&self, id: Option<i64>) {
        if id.is_some() {
            self.coll.borrow_mut().current = id;
        }
        if let Some(app) = self.app() {
            app.global::<crate::Nav>().invoke_go(crate::Page::Collections);
        }
        self.refresh_collections_page();
    }

    /// Jeux d'une collection, dans l'ordre choisi (jeux masqués compris).
    pub(crate) fn ordered_collection(&self, id: i64) -> Vec<GameId> {
        let positions = self.db().collection_positions(id).unwrap_or_default();
        let lib = self.lib.borrow();
        let present: Vec<(GameId, i64)> = positions.into_iter().filter(|(g, _)| lib.get(*g).is_some()).collect();
        coll::ordered_members(&present, |g| lib.get(g).map(|x| x.sort_key.clone()).unwrap_or_default())
    }

    pub(crate) fn refresh_collections_page(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<CollUi>();
        let cols = self.collections.borrow().clone();
        let current = {
            let mut st = self.coll.borrow_mut();
            if !st.current.is_some_and(|id| cols.iter().any(|c| c.id == id)) {
                st.current = cols.first().map(|c| c.id);
            }
            st.current
        };
        ui.set_collections(ModelRc::new(VecModel::from(self.coll_rows(&cols))));
        let suggestions: Vec<SeriesRow> = self
            .series_suggestions()
            .into_iter()
            .take(12)
            .map(|s| {
                let lib = self.lib.borrow();
                let sample: Vec<&str> = s.games.iter().take(3).filter_map(|g| lib.get(*g).map(|x| x.name.as_str())).collect();
                SeriesRow { key: s.key.as_str().into(), name: s.name.as_str().into(), count: s.games.len() as i32, sample: sample.join(" · ").into() }
            })
            .collect();
        ui.set_suggestions(ModelRc::new(VecModel::from(suggestions)));

        let Some(c) = current.and_then(|id| cols.iter().find(|c| c.id == id).cloned()) else {
            ui.set_current(-1);
            ui.set_games(ModelRc::default());
            self.coll.borrow_mut().order.clear();
            return;
        };
        let order = self.ordered_collection(c.id);
        ui.set_current(c.id as i32);
        ui.set_name(c.name.as_str().into());
        ui.set_color(collection_color(&c));
        ui.set_color_hex(c.color.clone().unwrap_or_default().into());
        ui.set_synced(coll::is_synced(&c.name));
        self.publish_collection_images(&c);
        let now = now_unix();
        let rows: Vec<CollGame> = {
            let lib = self.lib.borrow();
            order
                .iter()
                .filter_map(|id| lib.get(*id))
                .map(|g| CollGame {
                    id: g.id as i32,
                    title: g.name.as_str().into(),
                    detail: game_detail(g).into(),
                    checked: g.installed,
                    card: card_for(g, &self.images, now),
                })
                .collect()
        };
        let n = rows.len() as i32;
        ui.set_games(ModelRc::new(VecModel::from(rows)));
        if ui.get_game_index() >= n {
            ui.set_game_index(n - 1);
        }
        if n == 0 {
            ui.set_grabbing(false);
        }
        self.coll.borrow_mut().order = order;
    }

    fn publish_collection_images(&self, c: &Collection) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<CollUi>();
        let load = |p: &Option<PathBuf>, kind: Kind| {
            p.as_ref().and_then(|p| self.images.get(ImageKey::remote(&p.to_string_lossy(), kind), Some(p.as_path())))
        };
        let logo = load(&c.logo, Kind::Logo);
        let banner = load(&c.banner, Kind::Banner);
        ui.set_has_logo(logo.is_some());
        ui.set_logo(logo.unwrap_or_default());
        ui.set_has_banner(banner.is_some());
        ui.set_banner(banner.unwrap_or_default());
    }

    /// Visuel d'une collection chargé : la page le reprend si elle est affichée.
    pub(crate) fn collection_image_loaded(&self) {
        if self.app().is_some_and(|a| a.global::<crate::Nav>().get_page() == crate::Page::Collections) {
            let current = self.coll.borrow().current;
            let c = current.and_then(|id| self.collections.borrow().iter().find(|c| c.id == id).cloned());
            if let Some(c) = c {
                self.publish_collection_images(&c);
            }
        }
    }

    fn current_collection(&self) -> Option<Collection> {
        let id = self.coll.borrow().current?;
        self.collections.borrow().iter().find(|c| c.id == id).cloned()
    }

    fn select_collection(&self, id: i64) {
        self.coll.borrow_mut().current = Some(id);
        if let Some(app) = self.app() {
            let ui = app.global::<CollUi>();
            ui.set_game_index(0);
            ui.set_grabbing(false);
        }
        self.refresh_collections_page();
    }

    fn select_collection_delta(&self, delta: i32) {
        let ids: Vec<i64> = self.collections.borrow().iter().map(|c| c.id).collect();
        if ids.is_empty() {
            return;
        }
        let cur = self.coll.borrow().current.and_then(|id| ids.iter().position(|x| *x == id)).unwrap_or(0) as i32;
        let next = (cur + delta).clamp(0, ids.len() as i32 - 1) as usize;
        if next as i32 != cur {
            self.sound(halcyon_audio::Cue::GameSelect);
            self.select_collection(ids[next]);
        }
    }

    fn rename_current_collection(&self, name: &str) {
        let Some(c) = self.current_collection() else { return };
        let name = name.trim();
        if name == c.name || coll::is_synced(&c.name) {
            return;
        }
        if coll::is_synced(name) {
            self.toast(tr!("Ce nom est réservé aux collections reprises de Steam."), true);
            self.refresh_collections_page();
            return;
        }
        let result = self.db().rename_collection(c.id, name);
        match result {
            Ok(()) => {
                self.toast(tr!("Collection renommée en « {name} »", name = name), false);
                self.collections_changed();
            }
            Err(e) => {
                self.toast(e.to_string(), true);
                self.refresh_collections_page();
            }
        }
    }

    fn set_collection_color(&self, hex: &str) {
        let Some(c) = self.current_collection() else { return };
        let color = Some(hex).filter(|h| coll::parse_color(h).is_some());
        let result = self.db().set_collection_style(c.id, color, c.logo.as_deref(), c.banner.as_deref());
        if let Err(e) = result {
            self.toast(tr!("Erreur : {e}", e = e), true);
            return;
        }
        self.sound(halcyon_audio::Cue::Setting);
        self.collections_changed();
    }

    /// Logo ou bannière : copié dans les données de Halcyon (le fichier d'origine peut
    /// ensuite être déplacé) ; chemin vide : visuel retiré.
    fn set_collection_image(&self, kind: &str, path: &str) {
        let Some(c) = self.current_collection() else { return };
        let path = path.trim().trim_matches('"');
        let dest: Option<PathBuf> = if path.is_empty() {
            None
        } else {
            let src = Path::new(path);
            if image::ImageReader::open(src).and_then(|r| r.with_guessed_format()).map(|r| r.format().is_none()).unwrap_or(true) {
                self.toast(tr!("Image illisible : {src}", src = src.display()), true);
                return;
            }
            let dir = self.paths.data_dir.join("custom-artwork");
            let ext = src.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_else(|| "img".into());
            let dest = dir.join(format!("collection-{}-{kind}-{}.{ext}", c.id, now_unix()));
            if let Err(e) = std::fs::create_dir_all(&dir).and_then(|_| std::fs::copy(src, &dest)) {
                self.toast(tr!("Copie impossible : {e}", e = e), true);
                return;
            }
            Some(dest)
        };
        let (logo, banner) = match kind {
            "logo" => (dest.clone(), c.banner.clone()),
            "banner" => (c.logo.clone(), dest.clone()),
            _ => return,
        };
        let result = self.db().set_collection_style(c.id, c.color.as_deref(), logo.as_deref(), banner.as_deref());
        if let Err(e) = result {
            self.toast(tr!("Erreur : {e}", e = e), true);
            return;
        }
        // L'ancien fichier copié par Halcyon n'est plus utilisé.
        let old = if kind == "logo" { c.logo } else { c.banner };
        if let Some(old) = old.filter(|o| o.starts_with(self.paths.data_dir.join("custom-artwork")) && Some(o) != dest.as_ref()) {
            let _ = std::fs::remove_file(old);
        }
        self.collections_changed();
    }

    fn move_collection(&self, delta: i32) {
        let Some(c) = self.current_collection() else { return };
        let mut ids: Vec<i64> = self.collections.borrow().iter().map(|c| c.id).collect();
        if coll::move_item(&mut ids, &c.id, delta).is_none() {
            return;
        }
        if let Err(e) = self.db().set_collections_order(&ids) {
            self.toast(tr!("Erreur : {e}", e = e), true);
            return;
        }
        self.sound(halcyon_audio::Cue::GameSelect);
        self.collections_changed();
    }

    fn delete_current_collection(&self) {
        let Some(c) = self.current_collection() else { return };
        let name = c.name.clone();
        self.confirm(
            &tr!("Supprimer la collection ?"),
            &tr!("La collection « {name} » sera supprimée. Les jeux restent dans la bibliothèque.", name = name),
            &tr!("Supprimer"),
            move |ctl| {
                if let Err(e) = ctl.db().delete_collection(c.id) {
                    ctl.toast(tr!("Erreur : {e}", e = e), true);
                    return;
                }
                let _ = ctl.db().log_activity(ActivityKind::Settings, None, &tr_later!("Collection « {name} » supprimée", name = name), "");
                ctl.toast(tr!("Collection « {name} » supprimée", name = name), false);
                ctl.coll.borrow_mut().current = None;
                ctl.collections_changed();
            },
        );
    }

    /// Déplace un jeu d'une place (boutons ↑↓, Ctrl+flèches, manette) ; l'ordre complet est
    /// alors enregistré.
    fn move_collection_game(&self, index: usize, delta: i32) {
        let Some(c) = self.current_collection() else { return };
        let mut order = self.coll.borrow().order.clone();
        let Some(id) = order.get(index).copied() else { return };
        let Some(to) = coll::move_item(&mut order, &id, delta) else { return };
        if let Err(e) = self.db().set_collection_order(c.id, &order) {
            self.toast(tr!("Erreur : {e}", e = e), true);
            return;
        }
        self.sound(halcyon_audio::Cue::GameSelect);
        if let Some(app) = self.app() {
            // Mise à jour en place : seules les deux lignes concernées changent.
            let ui = app.global::<CollUi>();
            let model = ui.get_games();
            if let (Some(a), Some(b)) = (model.row_data(index), model.row_data(to)) {
                if to.abs_diff(index) == 1 {
                    model.set_row_data(index, b);
                    model.set_row_data(to, a);
                } else {
                    self.refresh_collections_page();
                }
            }
            ui.set_game_index(to as i32);
        }
        self.coll.borrow_mut().order = order;
        if self.bp.borrow().active {
            self.rebuild_bp_rows();
        }
    }

    fn remove_collection_game(&self, index: usize) {
        let Some(c) = self.current_collection() else { return };
        if coll::is_synced(&c.name) {
            return;
        }
        let Some(id) = self.coll.borrow().order.get(index).copied() else { return };
        if let Err(e) = self.db().set_in_collection(c.id, id, false) {
            self.toast(tr!("Erreur : {e}", e = e), true);
            return;
        }
        let name = self.lib.borrow().get(id).map(|g| g.name.clone()).unwrap_or_default();
        self.toast(tr!("« {game} » retiré de « {name} »", game = name, name = c.name), false);
        self.collections_changed();
    }

    /// Ouvre la page Collections depuis la bibliothèque : collection filtrée si c'en est une.
    pub(crate) fn open_collections_from_library(&self) {
        let id = self.app().and_then(|app| {
            let i = app.global::<LibraryUi>().get_collection() as usize;
            i.checked_sub(1).and_then(|i| self.collections.borrow().get(i).map(|c| c.id))
        });
        self.open_collections_page(id);
    }
}

/// Nom proposé d'après la recherche : « sonic » → « Sonic » (casse conservée si le
/// joueur a déjà tapé des majuscules).
fn title_case(s: &str) -> String {
    if s.chars().any(char::is_uppercase) || s.contains(':') {
        return s.to_string();
    }
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().chain(c).collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// « Steam · installé » : source (ou plateforme) et état, sous le titre d'une ligne.
fn game_detail(g: &halcyon_core::Game) -> String {
    let source = match &g.platform {
        Some(p) => crate::models::platform_names(p).1,
        None => g.source.label().to_string(),
    };
    if g.installed {
        source
    } else {
        tr!("{source} · non installé", source = source)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_follow_the_search() {
        assert_eq!(super::title_case("sonic"), "Sonic");
        assert_eq!(super::title_case("resident evil"), "Resident Evil");
        assert_eq!(super::title_case("GTA"), "GTA");
        assert_eq!(super::title_case("source:gog"), "source:gog");
    }
}
