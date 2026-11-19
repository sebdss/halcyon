//! Étagère : seconde vue du mode console (1.0.0-alpha.6).
//!
//! Une étagère par collection, plus « Récents », « Favoris » et « Tous ». Gauche / droite
//! parcourent les jeux, haut / bas changent d'étagère ; A joue, X met en favori, Select ouvre
//! le menu du jeu. La vue choisie est mémorisée (`Settings::console_view`).
//!
//! Légèreté : l'interface ne reçoit qu'une fenêtre d'environ 21 boîtiers autour de la
//! sélection, dans des **emplacements fixes** (le jeu d'indice `i` occupe l'emplacement
//! `i % 21`) : un boîtier qui reste dans la fenêtre garde son élément Slint, ses animations
//! ne repartent pas de zéro et rien n'est créé hors de l'écran. Les jaquettes sont décodées
//! par les fils du cache d'images ; seule celle du boîtier sélectionné l'est en grand
//! (`Kind::CoverLarge`, mise en cache sur disque comme l'ambiance).

use crate::controller::Controller;
use crate::images::{ImageKey, Kind};
use crate::models::{placeholder_colors, platform_names};
use crate::{ShelfBox, ShelfUi};
use halcyon_core::{Game, GameId, GameSource};
use slint::{Color, ComponentHandle, Model, ModelRc, VecModel};
use std::path::PathBuf;
use std::rc::Rc;

/// Emplacements de la fenêtre glissante (boîtiers transmis à l'interface).
pub const SLOTS: usize = 21;
/// Jeux de l'étagère « Récents ».
const RECENT_MAX: usize = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShelfKind {
    Recent,
    Favorites,
    Collection(i64),
    All,
}

#[derive(Clone, Debug)]
pub struct ShelfDef {
    pub kind: ShelfKind,
    pub name: String,
    pub color: Color,
    pub logo: Option<PathBuf>,
    pub games: Vec<GameId>,
}

/// État de l'étagère (dans l'état du mode console).
#[derive(Default)]
pub struct ShelfState {
    pub shelves: Vec<ShelfDef>,
    pub shelf: usize,
    /// Position de la sélection dans chaque étagère (retrouvée en y revenant).
    pub pos: Vec<usize>,
    pub boxes: Rc<VecModel<ShelfBox>>,
    pub next: Rc<VecModel<ShelfBox>>,
    /// Jeu affiché en arrière-plan et calque au premier plan (fondu enchaîné).
    pub bg_game: Option<GameId>,
    pub bg_a_front: bool,
    /// Changement d'arrière-plan différé : pendant un défilement rapide, le fond (plein écran,
    /// coûteux à redessiner en rendu logiciel) ne change qu'une fois la sélection posée.
    pub bg_timer: slint::Timer,
    pub bg_pending: Option<GameId>,
    /// Boîtier sélectionné retourné (dos visible) ; remis de face au moindre déplacement.
    pub flipped: bool,
    /// Halo : quelques respirations après un déplacement, puis immobile (aucun dessin au repos).
    pub breath_timer: slint::Timer,
    pub breaths: u8,
    pub breath_game: Option<GameId>,
}

/// Durée d'une demi-respiration du halo et nombre de demi-respirations après un déplacement.
const BREATH: std::time::Duration = std::time::Duration::from_millis(1700);
const BREATHS: u8 = 6;

/// Délai avant de changer l'arrière-plan après un déplacement.
const BG_DELAY: std::time::Duration = std::time::Duration::from_millis(220);

/// Fenêtre de `slots` indices autour de `focus` dans une liste de `len` éléments.
pub fn window(focus: usize, len: usize, slots: usize) -> std::ops::Range<usize> {
    if len <= slots {
        return 0..len;
    }
    let half = slots / 2;
    let lo = focus.saturating_sub(half).min(len - slots);
    lo..lo + slots
}

/// Couleur de la bande de la tranche : plateforme émulée (teinte stable d'après son
/// identifiant), sinon couleur propre à la source.
pub fn band_color(g: &Game) -> Color {
    if let Some(p) = &g.platform {
        let (a, _) = placeholder_colors(p);
        return a;
    }
    let rgb = match g.source {
        GameSource::Steam => (0x2a, 0x47, 0x5e),
        GameSource::Epic => (0x3a, 0x3a, 0x3a),
        GameSource::Gog => (0x86, 0x32, 0x8a),
        GameSource::Ubisoft => (0x00, 0x70, 0xd0),
        GameSource::Ea => (0xc8, 0x3a, 0x3a),
        GameSource::Xbox => (0x10, 0x7c, 0x10),
        GameSource::BattleNet => (0x14, 0x6e, 0xc8),
        GameSource::Riot => (0xb8, 0x30, 0x30),
        GameSource::Amazon => (0xc7, 0x7a, 0x00),
        GameSource::Itch => (0xd8, 0x4a, 0x4a),
        GameSource::Minecraft => (0x5b, 0x87, 0x31),
        GameSource::Emulated => (0x5c, 0x6b, 0x7a),
        GameSource::Standalone | GameSource::Manual => return placeholder_colors(&g.name).0,
    };
    Color::from_rgb_u8(rgb.0, rgb.1, rgb.2)
}

impl Controller {
    /// Étagères depuis la bibliothèque en mémoire : Récents, Favoris, chaque collection
    /// (dans l'ordre choisi), Tous. Les étagères vides sont omises.
    pub(crate) fn rebuild_shelves(&self) {
        let theme_accent = Color::from_rgb_u8(0x3d, 0x9b, 0xe9);
        let theme_warm = Color::from_rgb_u8(0xc2, 0x5a, 0x1c);
        let neutral = Color::from_rgb_u8(0x5c, 0x6b, 0x7a);
        let mut shelves: Vec<ShelfDef> = Vec::new();
        {
            let lib = self.lib.borrow();
            let mut visible: Vec<&Game> = lib.games.iter().filter(|g| !g.hidden).collect();
            visible.sort_by(|a, b| a.sort_key.cmp(&b.sort_key));
            let mut recent: Vec<&Game> = visible.iter().copied().filter(|g| g.last_played.is_some()).collect();
            recent.sort_by_key(|g| std::cmp::Reverse(g.last_played));
            recent.truncate(RECENT_MAX);
            let ids = |v: &[&Game]| v.iter().map(|g| g.id).collect::<Vec<_>>();
            shelves.push(ShelfDef { kind: ShelfKind::Recent, name: tr!("Récents"), color: theme_accent, logo: None, games: ids(&recent) });
            let fav: Vec<&Game> = visible.iter().copied().filter(|g| g.favorite).collect();
            shelves.push(ShelfDef { kind: ShelfKind::Favorites, name: tr!("Favoris"), color: theme_warm, logo: None, games: ids(&fav) });
            shelves.push(ShelfDef { kind: ShelfKind::All, name: tr!("Tous"), color: neutral, logo: None, games: ids(&visible) });
        }
        // Collections, entre « Favoris » et « Tous ».
        let cols = self.collections.borrow().clone();
        let all = shelves.pop();
        for c in cols {
            let order = self.ordered_collection(c.id);
            let games: Vec<GameId> = {
                let lib = self.lib.borrow();
                order.into_iter().filter(|g| lib.get(*g).is_some_and(|x| !x.hidden)).collect()
            };
            shelves.push(ShelfDef {
                kind: ShelfKind::Collection(c.id),
                color: crate::ctl_collections::collection_color(&c),
                name: c.name,
                logo: c.logo,
                games,
            });
        }
        shelves.extend(all);
        shelves.retain(|s| !s.games.is_empty());

        let mut bp = self.bp.borrow_mut();
        let st = &mut bp.shelf;
        // Même étagère et même jeu qu'avant si possible.
        let current = st.shelves.get(st.shelf).map(|s| (s.kind, s.games.get(st.pos.get(st.shelf).copied().unwrap_or(0)).copied()));
        let old: Vec<(ShelfKind, usize)> = st.shelves.iter().zip(&st.pos).map(|(s, p)| (s.kind, *p)).collect();
        st.pos = shelves
            .iter()
            .map(|s| old.iter().find(|(k, _)| *k == s.kind).map(|(_, p)| (*p).min(s.games.len().saturating_sub(1))).unwrap_or(0))
            .collect();
        st.shelf = current.and_then(|(k, _)| shelves.iter().position(|s| s.kind == k)).unwrap_or(0);
        if let Some((_, Some(game))) = current {
            if let Some(p) = shelves.get(st.shelf).and_then(|s| s.games.iter().position(|g| *g == game)) {
                st.pos[st.shelf] = p;
            }
        }
        st.shelves = shelves;
    }

    /// Jeu sélectionné sur l'étagère.
    pub(crate) fn shelf_focused(&self) -> Option<GameId> {
        let bp = self.bp.borrow();
        let st = &bp.shelf;
        st.shelves.get(st.shelf).and_then(|s| s.games.get(st.pos.get(st.shelf).copied().unwrap_or(0))).copied()
    }

    pub(crate) fn shelf_box(&self, g: &Game, index: usize) -> ShelfBox {
        let cover = self.images.get(ImageKey { game: g.id, kind: Kind::Cover }, g.artwork.cover.as_deref());
        let (a, b) = placeholder_colors(&g.name);
        let spine = self.case_spine(g);
        let mark = match &g.platform {
            Some(p) => platform_names(p).0,
            None => g.source.label().to_string(),
        };
        ShelfBox {
            index: index as i32,
            id: g.id as i32,
            title: g.name.as_str().into(),
            has_cover: cover.is_some(),
            cover: cover.unwrap_or_default(),
            color_a: a,
            color_b: b,
            initials: halcyon_core::util::initials(&g.name).into(),
            band: band_color(g),
            mark: mark.into(),
            installed: g.installed,
            favorite: g.favorite,
            has_spine: spine.is_some(),
            spine: spine.unwrap_or_default(),
            fresh: false,
        }
    }

    /// Remplit les emplacements fixes : seuls ceux dont le contenu change sont mis à jour.
    fn fill_slots(&self, model: &VecModel<ShelfBox>, games: &[GameId], focus: usize) {
        self.fill_slots_n(model, games, focus, SLOTS, None);
    }

    /// Variante à `slots` emplacements ; `fresh_after` : jeux ajoutés après cet horodatage
    /// marqués nouveaux (musée).
    pub(crate) fn fill_slots_n(&self, model: &VecModel<ShelfBox>, games: &[GameId], focus: usize, slots: usize, fresh_after: Option<i64>) {
        let range = window(focus, games.len(), slots);
        let mut wanted: Vec<ShelfBox> = vec![ShelfBox { index: -1, ..Default::default() }; slots];
        {
            let lib = self.lib.borrow();
            for i in range {
                if let Some(g) = games.get(i).and_then(|id| lib.get(*id)) {
                    let mut b = self.shelf_box(g, i);
                    b.fresh = fresh_after.is_some_and(|t| g.added_at > t);
                    wanted[i % slots] = b;
                }
            }
        }
        if model.row_count() != slots {
            model.set_vec(wanted);
            return;
        }
        for (k, b) in wanted.into_iter().enumerate() {
            if model.row_data(k).as_ref() != Some(&b) {
                model.set_row_data(k, b);
            }
        }
    }

    /// Publie l'étagère : plaque, boîtiers, aperçu de l'étagère suivante, arrière-plan.
    pub(crate) fn refresh_shelf(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<ShelfUi>();
        let (shelf, next, index, count, focus, next_focus, boxes, next_model) = {
            let bp = self.bp.borrow();
            let st = &bp.shelf;
            (
                st.shelves.get(st.shelf).cloned(),
                st.shelves.get(st.shelf + 1).cloned(),
                st.shelf,
                st.shelves.len(),
                st.pos.get(st.shelf).copied().unwrap_or(0),
                st.pos.get(st.shelf + 1).copied().unwrap_or(0),
                st.boxes.clone(),
                st.next.clone(),
            )
        };
        if ui.get_boxes().row_count() != SLOTS {
            ui.set_boxes(ModelRc::from(boxes.clone()));
            ui.set_next_boxes(ModelRc::from(next_model.clone()));
        }
        ui.set_shelf_count(count as i32);
        ui.set_shelf_index(index as i32);
        let Some(shelf) = shelf else {
            boxes.set_vec(Vec::new());
            next_model.set_vec(Vec::new());
            ui.set_name("".into());
            return;
        };
        ui.set_name(shelf.name.as_str().into());
        ui.set_color(shelf.color);
        ui.set_count(shelf.games.len() as i32);
        let logo = shelf.logo.as_ref().and_then(|p| self.images.get(ImageKey::remote(&p.to_string_lossy(), Kind::Logo), Some(p.as_path())));
        ui.set_has_logo(logo.is_some());
        ui.set_logo(logo.unwrap_or_default());
        {
            let bp = self.bp.borrow();
            let prev = index.checked_sub(1).and_then(|i| bp.shelf.shelves.get(i)).map(|s| s.name.clone()).unwrap_or_default();
            ui.set_prev_name(prev.into());
        }
        ui.set_next_name(next.as_ref().map(|s| s.name.clone()).unwrap_or_default().into());
        self.fill_slots(&boxes, &shelf.games, focus);
        ui.set_focus(focus as i32);
        ui.set_flipped(self.bp.borrow().shelf.flipped);
        match &next {
            Some(n) => {
                self.fill_slots(&next_model, &n.games, next_focus);
                ui.set_next_focus(next_focus as i32);
                ui.set_next_color(n.color);
            }
            None => next_model.set_vec(Vec::new()),
        }

        // Jaquette du boîtier sélectionné en grand, et arrière-plan en fondu enchaîné.
        let game = shelf.games.get(focus).and_then(|id| self.lib.borrow().get(*id).cloned());
        let Some(g) = game else { return };
        let large = self.images.get(ImageKey { game: g.id, kind: Kind::CoverLarge }, g.artwork.cover.as_deref());
        ui.set_has_focus_cover(large.is_some());
        ui.set_focus_cover(large.unwrap_or_default());
        let back = self.case_back(&g);
        ui.set_has_focus_back(back.is_some());
        ui.set_focus_back(back.unwrap_or_default());
        self.shelf_breathe_start(g.id);
        let (bg, ambient) = match &g.artwork.banner {
            Some(p) => (self.images.get(ImageKey { game: g.id, kind: Kind::Banner }, Some(p)), false),
            None => (self.images.get(ImageKey { game: g.id, kind: Kind::Ambient }, g.artwork.cover.as_deref()), true),
        };
        let front = {
            let mut bp = self.bp.borrow_mut();
            let st = &mut bp.shelf;
            if st.bg_game != Some(g.id) {
                // Premier fond : tout de suite ; ensuite, une fois la sélection posée.
                if st.bg_game.is_some() {
                    let id = g.id;
                    if st.bg_pending != Some(id) || !st.bg_timer.running() {
                        st.bg_pending = Some(id);
                        st.bg_timer.start(slint::TimerMode::SingleShot, BG_DELAY, move || {
                            crate::controller::post(move |c| c.shelf_bg_commit(id));
                        });
                    }
                    return;
                }
                st.bg_game = Some(g.id);
                st.bg_a_front = !st.bg_a_front;
            }
            st.bg_a_front
        };
        let img = bg.unwrap_or_default();
        if front {
            ui.set_bg_a(img);
            ui.set_bg_a_ambient(ambient);
        } else {
            ui.set_bg_b(img);
            ui.set_bg_b_ambient(ambient);
        }
        ui.set_bg_a_front(front);
    }

    /// Le halo respire quelques fois quand la sélection change (mouvement complet et effets
    /// seulement), puis reste immobile.
    fn shelf_breathe_start(&self, id: GameId) {
        let full = self.app().is_some_and(|a| {
            let t = a.global::<crate::Theme>();
            t.get_motion() == 0 && t.get_effects()
        });
        let mut bp = self.bp.borrow_mut();
        let st = &mut bp.shelf;
        if st.breath_game == Some(id) {
            return;
        }
        st.breath_game = Some(id);
        st.breaths = 0;
        if !full {
            st.breath_timer.stop();
            return;
        }
        st.breath_timer.start(slint::TimerMode::Repeated, BREATH, || {
            crate::controller::post(|c| c.shelf_breathe_tick());
        });
    }

    pub(crate) fn shelf_breathe_tick(&self) {
        let Some(app) = self.app() else { return };
        let on = {
            let mut bp = self.bp.borrow_mut();
            let active = bp.active;
            let st = &mut bp.shelf;
            st.breaths += 1;
            if st.breaths >= BREATHS || !active {
                st.breath_timer.stop();
                false
            } else {
                st.breaths % 2 == 1
            }
        };
        app.global::<ShelfUi>().set_breathe(on);
    }

    /// Fin du délai : le fond passe au jeu sélectionné s'il l'est toujours (fondu enchaîné).
    pub(crate) fn shelf_bg_commit(&self, id: GameId) {
        if !self.bp.borrow().active || self.shelf_focused() != Some(id) {
            return;
        }
        {
            let mut bp = self.bp.borrow_mut();
            if bp.shelf.bg_game == Some(id) {
                return;
            }
            bp.shelf.bg_game = Some(id);
            bp.shelf.bg_a_front = !bp.shelf.bg_a_front;
        }
        self.refresh_shelf();
    }

    /// Navigation sur l'étagère. Renvoie `true` si l'action a été traitée.
    pub(crate) fn shelf_input(self: &Rc<Self>, action: &str) -> bool {
        let mut play: Option<GameId> = None;
        let mut sheet: Option<GameId> = None;
        let mut favorite: Option<GameId> = None;
        let mut options: Option<GameId> = None;
        let mut to_bar = false;
        {
            let mut bp = self.bp.borrow_mut();
            let st = &mut bp.shelf;
            if st.shelves.is_empty() {
                if action == "up" {
                    drop(bp);
                    self.bp_set_zone(0);
                    return true;
                }
                return false;
            }
            let n = st.shelves.len();
            let len = st.shelves[st.shelf].games.len();
            let cur = st.pos[st.shelf].min(len.saturating_sub(1));
            let focused = st.shelves[st.shelf].games.get(cur).copied();
            let was_flipped = st.flipped;
            if action != "flip" && action != "context" {
                st.flipped = false;
            }
            match action {
                "flip" => st.flipped = !st.flipped,
                // B remet d'abord le boîtier de face.
                "back" if was_flipped => {}
                "left" => st.pos[st.shelf] = cur.saturating_sub(1),
                "right" => st.pos[st.shelf] = (cur + 1).min(len.saturating_sub(1)),
                "up" if st.shelf == 0 => to_bar = true,
                "up" => st.shelf -= 1,
                "down" => st.shelf = (st.shelf + 1).min(n - 1),
                "accept" => play = focused,
                "fav" => favorite = focused,
                "context" => options = focused,
                a => {
                    let parse = |p: &str| a.strip_prefix(p).and_then(|v| v.parse::<usize>().ok());
                    if let Some(i) = parse("shelf-hover:") {
                        st.pos[st.shelf] = i.min(len.saturating_sub(1));
                    } else if let Some(i) = parse("shelf-click:") {
                        if i == cur {
                            sheet = focused;
                        } else {
                            st.pos[st.shelf] = i.min(len.saturating_sub(1));
                        }
                    } else if let Some(i) = parse("shelf-play:") {
                        st.pos[st.shelf] = i.min(len.saturating_sub(1));
                        play = st.shelves[st.shelf].games.get(i).copied();
                    } else {
                        return false;
                    }
                }
            }
        }
        if to_bar {
            self.bp_set_zone(0);
            return true;
        }
        if let Some(id) = favorite {
            self.toggle_favorite(id);
            self.rebuild_bp_rows();
        }
        if let Some(id) = options {
            self.bp_open_options(id);
        }
        if let Some(id) = sheet {
            self.bp_open_sheet(id);
        }
        self.refresh_bp();
        if let Some(id) = play {
            if self.lib.borrow().get(id).is_some_and(|g| g.installed) {
                self.play(id);
            } else {
                // Non installé : la fiche dit pourquoi (et propose d'installer s'il le peut).
                self.sound(halcyon_audio::Cue::Error);
                self.bp_open_sheet(id);
                self.refresh_bp();
            }
        }
        true
    }

    /// Vue suivante du mode console : rangée → étagère → musée → rangée ; choix mémorisé.
    pub(crate) fn toggle_console_view(&self) {
        let view = {
            let mut s = self.settings.borrow_mut();
            s.console_view = (s.console_view + 1) % 3;
            s.console_view
        };
        self.bp.borrow_mut().shelf.flipped = false;
        self.save_settings();
        if let Some(app) = self.app() {
            app.global::<ShelfUi>().set_view(i32::from(view));
        }
        if view == 2 {
            self.museum_enter();
        }
        self.sound(halcyon_audio::Cue::Mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sliding_window_stays_in_bounds() {
        assert_eq!(window(0, 5, 21), 0..5);
        assert_eq!(window(0, 500, 21), 0..21);
        assert_eq!(window(10, 500, 21), 0..21);
        assert_eq!(window(11, 500, 21), 1..22);
        assert_eq!(window(250, 500, 21), 240..261);
        assert_eq!(window(499, 500, 21), 479..500);
        // Chaque indice de la fenêtre a son propre emplacement.
        let slots: std::collections::HashSet<usize> = window(250, 500, SLOTS).map(|i| i % SLOTS).collect();
        assert_eq!(slots.len(), SLOTS);
    }
}
