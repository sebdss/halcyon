//! Musée : troisième vue du mode console (1.0.0-alpha.8).
//!
//! Un mur d'étagères : une rangée par plateforme (chaque source de jeux PC, chaque système
//! émulé), les boîtiers tranche vers l'avant. Haut / bas changent de rangée, gauche / droite de
//! boîtier ; A joue, X met en favori, Y retourne le boîtier, Select ouvre le menu du jeu, L
//! change l'éclairage. Les jeux ajoutés depuis la dernière visite brillent.
//!
//! Légèreté : au plus [`ROWS`] rangées sont transmises à l'interface, chacune avec une fenêtre
//! glissante de [`CASES`] boîtiers dans des emplacements fixes (comme l'étagère) ; rien n'est
//! créé pour les rangées et les boîtiers hors de l'écran.

use crate::controller::Controller;
use crate::ctl_shelf::band_color;
use crate::models::platform_names;
use crate::{MuseumRow, MuseumUi, ShelfBox, ShelfUi};
use halcyon_core::{Game, GameId};
use slint::{Color, ComponentHandle, Model, ModelRc, VecModel};
use std::rc::Rc;

/// Rangées transmises à l'interface (trois visibles, une en cours de défilement).
pub const ROWS: usize = 4;
/// Boîtiers par rangée transmis à l'interface (environ 45 tranches tiennent en largeur).
pub const CASES: usize = 56;

#[derive(Clone, Debug)]
pub struct RowDef {
    pub name: String,
    pub color: Color,
    pub games: Vec<GameId>,
    pub playtime: u64,
}

#[derive(Default)]
pub struct MuseumState {
    pub rows: Vec<RowDef>,
    pub row: usize,
    pub pos: Vec<usize>,
    /// Emplacements fixes des rangées et de leurs boîtiers.
    pub slots: Vec<Rc<VecModel<ShelfBox>>>,
    pub model: Rc<VecModel<MuseumRow>>,
    pub flipped: bool,
    /// Dernière visite avant celle-ci : les jeux ajoutés depuis brillent.
    pub fresh_after: i64,
}

/// Clé de rangée d'un jeu : système émulé, sinon source.
fn row_key(g: &Game) -> String {
    match &g.platform {
        Some(p) => platform_names(p).0,
        None => g.source.label().to_string(),
    }
}

/// Regroupe des jeux (déjà triés) par clé, rangées triées par nom (casse ignorée), l'ordre des
/// jeux conservé dans chaque rangée.
pub fn group_rows<K: Clone + Ord, T: Clone>(items: &[(K, T)]) -> Vec<(K, Vec<T>)> {
    let mut rows: std::collections::BTreeMap<K, Vec<T>> = std::collections::BTreeMap::new();
    for (k, t) in items {
        rows.entry(k.clone()).or_default().push(t.clone());
    }
    rows.into_iter().collect()
}

impl Controller {
    /// Rangées depuis la bibliothèque en mémoire (jeux visibles, par plateforme).
    pub(crate) fn rebuild_museum(&self) {
        let rows: Vec<RowDef> = {
            let lib = self.lib.borrow();
            let mut visible: Vec<&Game> = lib.games.iter().filter(|g| !g.hidden).collect();
            visible.sort_by(|a, b| a.sort_key.cmp(&b.sort_key));
            let keyed: Vec<(String, GameId)> = visible.iter().map(|g| (row_key(g).to_lowercase(), g.id)).collect();
            group_rows(&keyed)
                .into_iter()
                .filter_map(|(_, games)| {
                    let first = lib.get(*games.first()?)?;
                    let playtime = games.iter().filter_map(|id| lib.get(*id)).map(|g| g.effective_playtime()).sum();
                    Some(RowDef { name: row_key(first), color: band_color(first), games, playtime })
                })
                .collect()
        };
        let mut bp = self.bp.borrow_mut();
        let st = &mut bp.museum;
        let current = st.rows.get(st.row).map(|r| (r.name.clone(), r.games.get(st.pos.get(st.row).copied().unwrap_or(0)).copied()));
        let old: Vec<(String, usize)> = st.rows.iter().zip(&st.pos).map(|(r, p)| (r.name.clone(), *p)).collect();
        st.pos = rows
            .iter()
            .map(|r| old.iter().find(|(n, _)| *n == r.name).map(|(_, p)| (*p).min(r.games.len().saturating_sub(1))).unwrap_or(0))
            .collect();
        st.row = current.as_ref().and_then(|(n, _)| rows.iter().position(|r| &r.name == n)).unwrap_or(0);
        if let Some((_, Some(game))) = current {
            if let Some(p) = rows.get(st.row).and_then(|r| r.games.iter().position(|g| *g == game)) {
                st.pos[st.row] = p;
            }
        }
        st.rows = rows;
        if st.slots.len() != ROWS {
            st.slots = (0..ROWS).map(|_| Rc::new(VecModel::default())).collect();
        }
    }

    /// Entrée dans le musée : les jeux ajoutés depuis la visite précédente brillent ; la visite
    /// est mémorisée. Les rangées se mettent en place (mouvement complet).
    pub(crate) fn museum_enter(&self) {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        let before = {
            let mut s = self.settings.borrow_mut();
            let b = s.museum_seen;
            s.museum_seen = now;
            b
        };
        self.save_settings();
        {
            let mut bp = self.bp.borrow_mut();
            // Première visite : rien n'est « nouveau ».
            bp.museum.fresh_after = if before == 0 { now } else { before };
            bp.museum.flipped = false;
        }
        self.rebuild_museum();
        let Some(app) = self.app() else { return };
        let ui = app.global::<MuseumUi>();
        ui.set_lighting(i32::from(self.settings.borrow().museum_lighting.min(2)));
        ui.set_settled(false);
        self.refresh_museum();
        crate::controller::post(|c| {
            if let Some(app) = c.app() {
                app.global::<MuseumUi>().set_settled(true);
            }
        });
    }

    /// Jeu sélectionné dans le musée.
    pub(crate) fn museum_focused(&self) -> Option<GameId> {
        let bp = self.bp.borrow();
        let st = &bp.museum;
        st.rows.get(st.row).and_then(|r| r.games.get(st.pos.get(st.row).copied().unwrap_or(0))).copied()
    }

    /// Publie les rangées visibles et leurs boîtiers.
    pub(crate) fn refresh_museum(&self) {
        self.gl3d_redraw();
        let Some(app) = self.app() else { return };
        let ui = app.global::<MuseumUi>();
        let (rows, row, pos, slots, model, flipped, fresh) = {
            let bp = self.bp.borrow();
            let st = &bp.museum;
            (st.rows.clone(), st.row, st.pos.clone(), st.slots.clone(), st.model.clone(), st.flipped, st.fresh_after)
        };
        if ui.get_rows().row_count() != ROWS || model.row_count() != ROWS {
            model.set_vec(vec![MuseumRow { index: -1, ..Default::default() }; ROWS]);
            ui.set_rows(ModelRc::from(model.clone()));
        }
        ui.set_row_count(rows.len() as i32);
        ui.set_row_focus(row as i32);
        ui.set_flipped(flipped);
        // Rangées autour de la sélection : celle d'avant, la sélection, deux après.
        let lo = row.saturating_sub(1).min(rows.len().saturating_sub(ROWS));
        let range = if rows.len() <= ROWS { 0..rows.len() } else { lo..lo + ROWS };
        let mut used = [false; ROWS];
        for i in range {
            let r = &rows[i];
            let slot = i % ROWS;
            used[slot] = true;
            let focus = pos.get(i).copied().unwrap_or(0);
            self.fill_slots_n(&slots[slot], &r.games, focus, CASES, Some(fresh));
            let entry = MuseumRow {
                index: i as i32,
                name: r.name.as_str().into(),
                color: r.color,
                count: r.games.len() as i32,
                info: museum_info(r).into(),
                focus: focus as i32,
                cases: ModelRc::from(slots[slot].clone()),
            };
            if model.row_data(slot).as_ref() != Some(&entry) {
                model.set_row_data(slot, entry);
            }
        }
        for (slot, u) in used.iter().enumerate() {
            if !u && model.row_data(slot).is_some_and(|r| r.index >= 0) {
                model.set_row_data(slot, MuseumRow { index: -1, ..Default::default() });
            }
        }
        // Dos du boîtier choisi (partagé avec l'étagère).
        if let Some(g) = self.museum_focused().and_then(|id| self.lib.borrow().get(id).cloned()) {
            let back = self.case_back(&g);
            let shelf = app.global::<ShelfUi>();
            shelf.set_has_focus_back(back.is_some());
            shelf.set_focus_back(back.unwrap_or_default());
        }
    }

    /// Navigation dans le musée. Renvoie `true` si l'action a été traitée.
    pub(crate) fn museum_input(self: &Rc<Self>, action: &str) -> bool {
        let mut play: Option<GameId> = None;
        let mut sheet: Option<GameId> = None;
        let mut favorite: Option<GameId> = None;
        let mut options: Option<GameId> = None;
        let mut to_bar = false;
        let mut lighting = false;
        {
            let mut bp = self.bp.borrow_mut();
            let st = &mut bp.museum;
            if st.rows.is_empty() {
                if action == "up" {
                    drop(bp);
                    self.bp_set_zone(0);
                    return true;
                }
                return false;
            }
            let n = st.rows.len();
            let len = st.rows[st.row].games.len();
            let cur = st.pos[st.row].min(len.saturating_sub(1));
            let focused = st.rows[st.row].games.get(cur).copied();
            let was_flipped = st.flipped;
            if action != "flip" && action != "context" && action != "lighting" {
                st.flipped = false;
            }
            match action {
                "flip" => st.flipped = !st.flipped,
                "back" if was_flipped => {}
                "lighting" => lighting = true,
                "left" => st.pos[st.row] = cur.saturating_sub(1),
                "right" => st.pos[st.row] = (cur + 1).min(len.saturating_sub(1)),
                "up" if st.row == 0 => to_bar = true,
                "up" => st.row -= 1,
                "down" => st.row = (st.row + 1).min(n - 1),
                "accept" => play = focused,
                "fav" => favorite = focused,
                "context" => options = focused,
                a => {
                    // « museum-click:<rangée>:<boîtier> » et « museum-play:… » (souris).
                    let parse = |p: &str| {
                        let rest = a.strip_prefix(p)?;
                        let (r, i) = rest.split_once(':')?;
                        Some((r.parse::<usize>().ok()?, i.parse::<usize>().ok()?))
                    };
                    if let Some((r, i)) = parse("museum-click:") {
                        let r = r.min(n - 1);
                        let i = i.min(st.rows[r].games.len().saturating_sub(1));
                        if r == st.row && i == cur {
                            sheet = focused;
                        } else {
                            st.row = r;
                            st.pos[r] = i;
                        }
                    } else if let Some((r, i)) = parse("museum-play:") {
                        let r = r.min(n - 1);
                        let i = i.min(st.rows[r].games.len().saturating_sub(1));
                        st.row = r;
                        st.pos[r] = i;
                        play = st.rows[r].games.get(i).copied();
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
        if lighting {
            let l = {
                let mut s = self.settings.borrow_mut();
                s.museum_lighting = (s.museum_lighting + 1) % 3;
                s.museum_lighting
            };
            self.save_settings();
            if let Some(app) = self.app() {
                app.global::<MuseumUi>().set_lighting(i32::from(l));
            }
            self.sound(halcyon_audio::Cue::Setting);
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
                self.sound(halcyon_audio::Cue::Error);
                self.bp_open_sheet(id);
                self.refresh_bp();
            }
        }
        true
    }
}

/// Ligne d'information de la plaque : nombre de jeux et temps de jeu cumulé.
fn museum_info(r: &RowDef) -> String {
    let count = trn!(r.games.len(), "{n} jeu", "{n} jeux");
    if r.playtime == 0 {
        count
    } else {
        format!("{count} · {}", halcyon_core::util::format_playtime(r.playtime))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_group_by_key_and_keep_game_order() {
        let items = vec![("steam", 3), ("snes", 1), ("steam", 1), ("gog", 9), ("snes", 2)];
        let rows = group_rows(&items);
        assert_eq!(rows, vec![("gog", vec![9]), ("snes", vec![1, 2]), ("steam", vec![3, 1])]);
        assert!(group_rows::<&str, i32>(&[]).is_empty());
    }
}
