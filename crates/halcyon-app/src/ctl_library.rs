//! Bibliothèque 0.4 : vues, sélection au clavier et à la manette, bande du carrousel.
//!
//! La sélection est mémorisée par identifiant de jeu : elle survit aux filtres, aux tris et
//! aux changements de vue. Le carrousel et la vue immersive ne reçoivent qu'une fenêtre de
//! jeux autour de la sélection, quelle que soit la taille de la bibliothèque.

use crate::controller::Controller;
use crate::images::{ImageKey, Kind};
use crate::models::{card_for, detail_for, Tracking};
use crate::{LibraryUi, StripItem};
use halcyon_core::model::GameId;
use halcyon_storage::now_unix;
use halcyon_storage::settings::LibraryView;
use slint::{ComponentHandle, ModelRc, VecModel};

/// Vues : grille, liste, carrousel, immersive, compacte.
pub const VIEW_GRID: i32 = 0;
pub const VIEW_LIST: i32 = 1;
pub const VIEW_CAROUSEL: i32 = 2;
pub const VIEW_IMMERSIVE: i32 = 3;
pub const VIEW_COMPACT: i32 = 4;
/// Jeux de part et d'autre de la sélection dans la bande.
const STRIP_RADIUS: usize = 10;

/// Nouvelle position après un déplacement (dx, dy), selon la vue et le nombre de colonnes.
/// `None` : rien de sélectionné → premier jeu.
pub fn step(current: Option<usize>, len: usize, view: i32, columns: usize, dx: i32, dy: i32) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let Some(cur) = current.filter(|c| *c < len) else { return Some(0) };
    let cols = columns.max(1) as i64;
    let delta: i64 = match view {
        VIEW_GRID | VIEW_COMPACT => i64::from(dx) + i64::from(dy) * cols,
        VIEW_LIST => i64::from(dy) + i64::from(dx),
        // Bandes horizontales : haut/bas sautent de 5.
        _ => i64::from(dx) + i64::from(dy) * 5,
    };
    let next = cur as i64 + delta;
    // En grille, une flèche verticale hors limites reste sur place (pas de saut de colonne).
    if matches!(view, VIEW_GRID | VIEW_COMPACT) && dy != 0 && (next < 0 || next >= len as i64) {
        let last_row_start = (len - 1) / cols as usize * cols as usize;
        return Some(if next < 0 { cur } else if cur < last_row_start { len - 1 } else { cur });
    }
    Some(next.clamp(0, len as i64 - 1) as usize)
}

impl Controller {
    fn view_mode(&self) -> i32 {
        self.app().map_or(VIEW_GRID, |a| a.global::<LibraryUi>().get_view_mode())
    }

    /// Position de la sélection dans la liste filtrée.
    pub(crate) fn selected_pos(&self) -> Option<usize> {
        let id = self.lib_selected.get()?;
        self.lib.borrow().pos_of.get(&id).copied()
    }

    pub(crate) fn select_pos(&self, pos: Option<usize>, scroll: bool) {
        let id = pos.and_then(|p| {
            let lib = self.lib.borrow();
            lib.filtered.get(p).map(|&i| lib.games[i].id)
        });
        self.lib_selected.set(id);
        self.publish_selection(scroll);
    }

    /// Republie la sélection après un filtrage : conservée si le jeu est toujours affiché.
    pub(crate) fn sync_selection(&self) {
        let view = self.view_mode();
        if self.selected_pos().is_none() {
            let strip = matches!(view, VIEW_CAROUSEL | VIEW_IMMERSIVE);
            let first = if strip { Some(0) } else { None };
            let id = first.and_then(|p| {
                let lib = self.lib.borrow();
                lib.filtered.get(p).map(|&i| lib.games[i].id)
            });
            self.lib_selected.set(id);
        }
        self.publish_selection(true);
    }

    fn publish_selection(&self, scroll: bool) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<LibraryUi>();
        let pos = self.selected_pos();
        let view = ui.get_view_mode();
        ui.set_selected(pos.map_or(-1, |p| p as i32));
        ui.set_selected_id(self.lib_selected.get().filter(|_| pos.is_some()).map_or(-1, |id| id as i32));
        if scroll {
            if let Some(p) = pos {
                let cols = self.lib.borrow().columns().max(1);
                let row = if matches!(view, VIEW_GRID | VIEW_COMPACT) { p / cols } else { p };
                ui.set_scroll_row(row as i32);
                ui.set_scroll_tick(ui.get_scroll_tick().wrapping_add(1));
            }
        }
        if matches!(view, VIEW_CAROUSEL | VIEW_IMMERSIVE) {
            self.publish_strip(pos);
        }
    }

    /// Fenêtre de jaquettes autour de la sélection et fiche du jeu sélectionné.
    fn publish_strip(&self, pos: Option<usize>) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<LibraryUi>();
        let now = now_unix();
        let (items, focus) = {
            let lib = self.lib.borrow();
            let Some(p) = pos else {
                ui.set_strip(ModelRc::default());
                return;
            };
            let lo = p.saturating_sub(STRIP_RADIUS);
            let hi = (p + STRIP_RADIUS + 1).min(lib.filtered.len());
            let items: Vec<StripItem> = (lo..hi)
                .map(|i| StripItem { index: i as i32, card: card_for(&lib.games[lib.filtered[i]], &self.images, now) })
                .collect();
            (items, lib.filtered.get(p).map(|&i| lib.games[i].clone()))
        };
        ui.set_strip(ModelRc::new(VecModel::from(items)));
        if let Some(g) = focus {
            ui.set_focus_game(detail_for(&g, &self.images, Tracking::default(), now));
            ui.set_focus_meta(self.meta_line(&g, now).into());
            // Sans bannière : ambiance floutée tirée de la jaquette.
            if g.artwork.banner.is_none() {
                let img = self.images.get(ImageKey { game: g.id, kind: Kind::Ambient }, g.artwork.cover.as_deref());
                ui.set_focus_ambient(img.unwrap_or_default());
            }
        }
    }

    /// Un visuel du jeu a changé : met à jour la bande s'il y figure.
    pub(crate) fn strip_game_changed(&self, id: GameId) {
        let view = self.view_mode();
        if !matches!(view, VIEW_CAROUSEL | VIEW_IMMERSIVE) {
            return;
        }
        let pos = self.selected_pos();
        let in_window = {
            let lib = self.lib.borrow();
            match (pos, lib.pos_of.get(&id)) {
                (Some(p), Some(&q)) => q + STRIP_RADIUS >= p && q <= p + STRIP_RADIUS,
                _ => false,
            }
        };
        if in_window {
            self.publish_strip(pos);
        }
    }

    pub(crate) fn move_selection(&self, dx: i32, dy: i32) {
        let (len, cols) = {
            let lib = self.lib.borrow();
            (lib.filtered.len(), lib.columns())
        };
        let before = self.selected_pos();
        let next = step(before, len, self.view_mode(), cols, dx, dy);
        if next != before {
            self.sound(halcyon_audio::Cue::GameSelect);
        }
        self.select_pos(next, true);
    }

    pub(crate) fn select_index(&self, i: i32) {
        let len = self.lib.borrow().filtered.len();
        if len > 0 {
            self.select_pos(Some(i.clamp(0, len as i32 - 1) as usize), true);
        }
    }

    pub(crate) fn activate_selection(self: &std::rc::Rc<Self>, play: bool) {
        let Some(id) = self.lib_selected.get().filter(|_| self.selected_pos().is_some()) else { return };
        if play {
            self.play(id);
        } else {
            self.open_game(id);
        }
    }

    pub(crate) fn on_view_changed(&self) {
        let Some(app) = self.app() else { return };
        let view = app.global::<LibraryUi>().get_view_mode().clamp(0, 4);
        {
            let mut s = self.settings.borrow_mut();
            s.appearance.library_view = view as u8;
            // Format 0.3 conservé (grille / liste) pour un retour en arrière éventuel.
            s.library_view = if view == VIEW_LIST { LibraryView::List } else { LibraryView::Grid };
        }
        app.global::<LibraryUi>().set_list_view(view == VIEW_LIST);
        self.save_settings();
        self.sync_selection();
    }

    pub(crate) fn set_card_size(&self, px: i32) {
        let px = px.clamp(120, 260) as u32;
        self.settings.borrow_mut().card_width = px;
        if let Some(app) = self.app() {
            app.global::<LibraryUi>().set_card_width(px as f32);
        }
        self.save_settings();
    }

    /// Vue enregistrée (0.4), ou reprise du réglage grille/liste de la 0.3.
    pub(crate) fn saved_view(&self) -> i32 {
        let s = self.settings.borrow();
        match s.appearance.library_view {
            0 if s.library_view == LibraryView::List => VIEW_LIST,
            v => i32::from(v.min(4)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_moves_by_rows_and_stays_in_bounds() {
        // 10 jeux, 4 colonnes : rangées 0-3, 4-7, 8-9.
        assert_eq!(step(None, 10, VIEW_GRID, 4, 1, 0), Some(0));
        assert_eq!(step(Some(1), 10, VIEW_GRID, 4, 0, 1), Some(5));
        assert_eq!(step(Some(1), 10, VIEW_GRID, 4, 0, -1), Some(1));
        assert_eq!(step(Some(6), 10, VIEW_GRID, 4, 0, 1), Some(9));
        assert_eq!(step(Some(9), 10, VIEW_GRID, 4, 0, 1), Some(9));
        assert_eq!(step(Some(3), 10, VIEW_GRID, 4, 1, 0), Some(4));
        assert_eq!(step(Some(9), 10, VIEW_GRID, 4, 1, 0), Some(9));
        assert_eq!(step(Some(0), 10, VIEW_GRID, 4, -1, 0), Some(0));
    }

    #[test]
    fn list_and_strip_views() {
        assert_eq!(step(Some(2), 10, VIEW_LIST, 1, 0, 1), Some(3));
        assert_eq!(step(Some(2), 10, VIEW_LIST, 1, 0, -5), Some(0));
        assert_eq!(step(Some(2), 10, VIEW_CAROUSEL, 1, 1, 0), Some(3));
        assert_eq!(step(Some(2), 10, VIEW_IMMERSIVE, 1, 0, 1), Some(7));
        assert_eq!(step(Some(8), 10, VIEW_CAROUSEL, 1, 0, 1), Some(9));
        assert_eq!(step(Some(3), 0, VIEW_GRID, 4, 1, 0), None);
        // Sélection devenue hors liste (filtre) : retour au début.
        assert_eq!(step(Some(30), 10, VIEW_GRID, 4, 1, 0), Some(0));
    }
}
