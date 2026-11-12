//! Contrôleur — mode Big Picture : état de navigation piloté par le clavier et la manette.

use crate::controller::Controller;
use crate::gamepad::PadAction;
use crate::models::{card_for, detail_for};
use crate::{AchievementRow, BpRow, BpUi, GameCard};
use halcyon_core::{Game, GameId};
use halcyon_storage::now_unix;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::rc::Rc;

/// Jaquettes affichées par rangée autour de l'élément actif.
const WINDOW: usize = 9;
/// Succès visibles dans la fiche.
const ACH_WINDOW: usize = 5;
const KEYS: &[&str] = &[
    "A", "Z", "E", "R", "T", "Y", "U", "I", "O", "P", "Q", "S", "D", "F", "G", "H", "J", "K", "L", "M", "W", "X", "C", "V",
    "B", "N", "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "␣", "⌫",
];

#[derive(Default)]
pub struct BigPicture {
    pub active: bool,
    view: u8,
    rows: Vec<(String, Vec<GameId>)>,
    row: usize,
    cols: Vec<usize>,
    game: Option<GameId>,
    button: usize,
    ach_index: usize,
    ach_count: usize,
    query: String,
    key: usize,
    in_results: bool,
    results: Vec<GameId>,
    result_col: usize,
    /// Ligne sélectionnée dans les réglages (vue 3).
    setting: usize,
}

impl Controller {
    pub(crate) fn enter_big_picture(self: &Rc<Self>) {
        let Some(app) = self.app() else { return };
        if !self.bp.borrow().active {
            self.sound(halcyon_audio::Cue::Mode);
        }
        {
            let mut bp = self.bp.borrow_mut();
            if bp.active {
                return;
            }
            bp.active = true;
            bp.view = 0;
        }
        self.rebuild_bp_rows();
        app.window().set_fullscreen(true);
        app.global::<BpUi>().set_active(true);
        app.global::<BpUi>().set_pad_connected(self.pad_connected.get());
        // Horloge mise à jour chaque minute, uniquement en Big Picture.
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
            app.global::<BpUi>().set_clock(local_clock().into());
        }
    }

    /// Construit les rangées : reprendre, favoris, bibliothèque, émulation, collections.
    pub(crate) fn rebuild_bp_rows(&self) {
        let lib = self.lib.borrow();
        let mut visible: Vec<&Game> = lib.games.iter().filter(|g| !g.hidden).collect();
        visible.sort_by(|a, b| a.sort_key.cmp(&b.sort_key));
        let mut recent: Vec<&Game> = visible.iter().copied().filter(|g| g.last_played.is_some()).collect();
        recent.sort_by_key(|g| std::cmp::Reverse(g.last_played));
        let ids = |v: &[&Game]| v.iter().map(|g| g.id).collect::<Vec<_>>();

        let mut rows: Vec<(String, Vec<GameId>)> = vec![
            ("Reprendre".into(), ids(&recent).into_iter().take(20).collect()),
            ("Favoris".into(), ids(&visible.iter().copied().filter(|g| g.favorite).collect::<Vec<_>>())),
            ("Installés".into(), ids(&visible.iter().copied().filter(|g| g.installed).collect::<Vec<_>>())),
            (
                "Émulation".into(),
                ids(&visible.iter().copied().filter(|g| g.source == halcyon_core::GameSource::Emulated).collect::<Vec<_>>()),
            ),
            ("Toute la bibliothèque".into(), ids(&visible)),
        ];
        drop(lib);
        for c in self.collections.borrow().iter() {
            if let Ok(members) = self.db().collection_members(c.id) {
                let list: Vec<GameId> = {
                    let lib = self.lib.borrow();
                    let mut v: Vec<&Game> = members.iter().filter_map(|id| lib.get(*id)).filter(|g| !g.hidden).collect();
                    v.sort_by(|a, b| a.sort_key.cmp(&b.sort_key));
                    v.iter().map(|g| g.id).collect()
                };
                rows.push((c.name.clone(), list));
            }
        }
        rows.retain(|(_, list)| !list.is_empty());
        let mut bp = self.bp.borrow_mut();
        bp.cols = rows.iter().enumerate().map(|(i, (_, l))| bp_clamp(&bp.cols, i, l.len())).collect();
        bp.row = bp.row.min(rows.len().saturating_sub(1));
        bp.rows = rows;
    }

    fn bp_focused(&self) -> Option<GameId> {
        let bp = self.bp.borrow();
        match bp.view {
            1 => bp.game,
            2 if bp.in_results => bp.results.get(bp.result_col).copied(),
            2 => None,
            _ => bp.rows.get(bp.row).and_then(|(_, ids)| ids.get(bp.cols[bp.row]).copied()),
        }
    }

    fn bp_cards(&self, ids: &[GameId], focus: usize) -> (Vec<GameCard>, i32) {
        let start = focus.saturating_sub(1);
        let lib = self.lib.borrow();
        let now = now_unix();
        let cards = ids.iter().skip(start).take(WINDOW).filter_map(|id| lib.get(*id)).map(|g| card_for(g, &self.images, now)).collect();
        (cards, (focus - start) as i32)
    }

    /// Pousse l'état de navigation vers l'interface.
    pub(crate) fn refresh_bp(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<BpUi>();
        let (view, rows, row, cols, button, ach_index, query, key, in_results, results, result_col) = {
            let bp = self.bp.borrow();
            (
                bp.view,
                bp.rows.clone(),
                bp.row,
                bp.cols.clone(),
                bp.button,
                bp.ach_index,
                bp.query.clone(),
                bp.key,
                bp.in_results,
                bp.results.clone(),
                bp.result_col,
            )
        };
        let models: Vec<BpRow> = rows
            .iter()
            .enumerate()
            .map(|(i, (title, ids))| {
                let (cards, focus) = self.bp_cards(ids, cols[i]);
                BpRow {
                    title: title.as_str().into(),
                    items: ModelRc::new(VecModel::from(cards)),
                    focus: if i == row { focus } else { 0 },
                    count: ids.len() as i32,
                }
            })
            .collect();
        ui.set_rows(ModelRc::new(VecModel::from(models)));
        ui.set_focus_row(row as i32);
        ui.set_view(view as i32);

        let focused = self.bp_focused().or_else(|| self.bp.borrow().game);
        let game = focused.and_then(|id| self.lib.borrow().get(id).cloned());
        if let Some(g) = &game {
            let now = now_unix();
            ui.set_focused(detail_for(g, &self.images, crate::models::Tracking::default(), now));
            ui.set_focused_meta(self.meta_line(g, now).into());
            let p = self.progress.borrow().get(&g.id).copied();
            ui.set_focused_ach(
                p.map(|p| format!("🏆 {} / {} succès · {:.0} %", p.unlocked, p.total, p.percent())).unwrap_or_default().into(),
            );
        }
        ui.set_game_button(button as i32);
        if view == 1 {
            if let Some(g) = &game {
                let list = self.db().load_achievements(g.id).unwrap_or_default();
                self.bp.borrow_mut().ach_count = list.len();
                let start = ach_index.saturating_sub(2).min(list.len().saturating_sub(ACH_WINDOW));
                let rows: Vec<AchievementRow> =
                    list.iter().skip(start).take(ACH_WINDOW).map(|a| self.achievement_row(a, &g.name)).collect();
                ui.set_achievements(ModelRc::new(VecModel::from(rows)));
                ui.set_ach_index(if button == 2 { (ach_index - start) as i32 } else { -1 });
            }
        }
        if view == 3 {
            ui.set_settings(ModelRc::new(VecModel::from(self.bp_settings_rows())));
            ui.set_setting_index(self.bp.borrow().setting as i32);
        }
        if view == 2 {
            ui.set_query(query.as_str().into());
            ui.set_keys(ModelRc::new(VecModel::from(KEYS.iter().map(|k| SharedString::from(*k)).collect::<Vec<_>>())));
            ui.set_key_index(key as i32);
            ui.set_in_results(in_results);
            let (cards, focus) = self.bp_cards(&results, result_col);
            ui.set_results(BpRow {
                title: if query.is_empty() { "Résultats".into() } else { format!("Résultats pour « {query} »").into() },
                items: ModelRc::new(VecModel::from(cards)),
                focus: if in_results { focus } else { 0 },
                count: results.len() as i32,
            });
        }
    }

    fn bp_settings_rows(&self) -> Vec<crate::KeyValue> {
        let a = self.settings.borrow().appearance.clone();
        let theme = self.all_themes().into_iter().find(|t| t.id == a.theme).map(|t| t.name).unwrap_or_default();
        let kv = |label: &str, value: String, adjustable: bool| crate::KeyValue {
            label: label.into(),
            value: value.into(),
            ratio: if adjustable { 1.0 } else { 0.0 },
        };
        vec![
            kv("Mode Performance", if self.performance_active() { "Activé" } else { "Désactivé" }.into(), true),
            kv("Mouvement", ["Complet", "Réduit", "Aucun"][usize::from(a.motion().min(2))].into(), true),
            kv("Taille du texte", format!("{:.0} %", a.text_scale * 100.0), true),
            kv("Thème", theme, true),
            kv("Mode du bureau", ["Standard", "Minimaliste", "Cinéma"][usize::from(a.ui_mode.min(2))].into(), true),
            kv("Analyser les sources", String::new(), false),
            kv("Synchroniser les succès", String::new(), false),
            kv("Paramètres complets", String::new(), false),
            kv("Quitter Big Picture", String::new(), false),
        ]
    }

    fn bp_settings_input(self: &Rc<Self>, action: &str) {
        const COUNT: usize = 9;
        let cur = self.bp.borrow().setting;
        let (index, dir): (usize, i32) = match action {
            "up" => {
                self.bp.borrow_mut().setting = cur.saturating_sub(1);
                return self.refresh_bp();
            }
            "down" => {
                self.bp.borrow_mut().setting = (cur + 1).min(COUNT - 1);
                return self.refresh_bp();
            }
            "back" => {
                self.bp.borrow_mut().view = 0;
                return self.refresh_bp();
            }
            "left" => (cur, -1),
            "right" | "accept" => (cur, 1),
            a if a.starts_with("setting:") => {
                let i = a[8..].parse::<usize>().unwrap_or(0).min(COUNT - 1);
                self.bp.borrow_mut().setting = i;
                (i, 1)
            }
            _ => return,
        };
        let a = self.settings.borrow().appearance.clone();
        let cycle = |v: i32, n: i32| (v + dir).rem_euclid(n);
        match index {
            0 => {
                let on = !self.performance_active();
                self.set_performance(on);
            }
            1 => self.set_motion(cycle(i32::from(a.motion()), 3)),
            2 => {
                const SCALES: [f32; 4] = [0.9, 1.0, 1.15, 1.3];
                let i = SCALES.iter().position(|s| (s - a.text_scale).abs() < 0.04).unwrap_or(1) as i32;
                self.set_text_scale(SCALES[cycle(i, 4) as usize]);
            }
            3 => {
                let themes = self.all_themes();
                let i = themes.iter().position(|t| t.id == a.theme).unwrap_or(0) as i32;
                self.select_theme(cycle(i, themes.len().max(1) as i32) as usize);
            }
            4 => self.set_ui_mode(cycle(i32::from(a.ui_mode), 3)),
            5 if action != "left" => self.start_scan(),
            6 if action != "left" => self.start_ach_sync(None, true),
            7 if action != "left" => {
                self.exit_big_picture();
                if let Some(app) = self.app() {
                    app.global::<crate::Nav>().invoke_go(crate::Page::Settings);
                }
                return;
            }
            8 if action != "left" => return self.exit_big_picture(),
            _ => {}
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
        if bp.results.is_empty() {
            bp.in_results = false;
        }
    }

    /// Action de navigation (clavier, manette ou bouton).
    pub(crate) fn bp_input(self: &Rc<Self>, action: &str) {
        if !self.bp.borrow().active {
            return;
        }
        if action == "exit" {
            self.exit_big_picture();
            return;
        }
        // Sons de navigation du mode console.
        let cue = match action {
            "up" | "down" | "left" | "right" => Some(halcyon_audio::Cue::GameSelect),
            "accept" => Some(halcyon_audio::Cue::Confirm),
            "back" => Some(halcyon_audio::Cue::Back),
            "search" | "settings" => Some(halcyon_audio::Cue::PanelOpen),
            _ => None,
        };
        if let Some(cue) = cue {
            self.sound(cue);
        }
        // Réglages (vue 3), pilotables à la manette seule.
        if action == "settings" {
            {
                let mut bp = self.bp.borrow_mut();
                bp.view = 3;
                bp.setting = 0;
            }
            self.refresh_bp();
            return;
        }
        if self.bp.borrow().view == 3 {
            self.bp_settings_input(action);
            return;
        }
        let view = self.bp.borrow().view;
        let mut play: Option<GameId> = None;
        let mut favorite: Option<GameId> = None;
        let mut search_changed = false;
        {
            let mut bp = self.bp.borrow_mut();
            match (view, action) {
                (_, "search") => {
                    bp.view = 2;
                    bp.in_results = false;
                }
                (0, "up") => bp.row = bp.row.saturating_sub(1),
                (0, "down") => bp.row = (bp.row + 1).min(bp.rows.len().saturating_sub(1)),
                (0, "left") => {
                    let r = bp.row;
                    if let Some(c) = bp.cols.get_mut(r) {
                        *c = c.saturating_sub(1);
                    }
                }
                (0, "right") => {
                    let r = bp.row;
                    let len = bp.rows.get(r).map(|(_, l)| l.len()).unwrap_or(0);
                    if let Some(c) = bp.cols.get_mut(r) {
                        *c = (*c + 1).min(len.saturating_sub(1));
                    }
                }
                (0, "accept") => {
                    let r = bp.row;
                    if let Some(id) = bp.rows.get(r).and_then(|(_, l)| l.get(bp.cols[r])).copied() {
                        bp.game = Some(id);
                        bp.view = 1;
                        bp.button = 0;
                        bp.ach_index = 0;
                    }
                }
                (0, "fav") => {
                    let r = bp.row;
                    favorite = bp.rows.get(r).and_then(|(_, l)| l.get(bp.cols[r])).copied();
                }
                (1, "left") => bp.button = bp.button.saturating_sub(1),
                (1, "right") => bp.button = (bp.button + 1).min(2),
                (1, "up") if bp.button == 2 => bp.ach_index = bp.ach_index.saturating_sub(1),
                (1, "down") if bp.button == 2 => bp.ach_index = (bp.ach_index + 1).min(bp.ach_count.saturating_sub(1)),
                (1, "accept") => match bp.button {
                    0 => play = bp.game,
                    1 => favorite = bp.game,
                    _ => {}
                },
                (1, "fav") => favorite = bp.game,
                (1, "back") => bp.view = if bp.query.is_empty() || bp.results.is_empty() { 0 } else { 2 },
                (2, "back") => {
                    bp.view = 0;
                    bp.in_results = false;
                }
                (2, a) if !bp.in_results => match a {
                    "left" => bp.key = bp.key.saturating_sub(1),
                    "right" => bp.key = (bp.key + 1).min(KEYS.len() - 1),
                    "up" => bp.key = bp.key.saturating_sub(10),
                    "down" => {
                        if bp.key + 10 < KEYS.len() {
                            bp.key += 10;
                        } else if !bp.results.is_empty() {
                            bp.in_results = true;
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
                (2, a) => match a {
                    "up" => bp.in_results = false,
                    "left" => bp.result_col = bp.result_col.saturating_sub(1),
                    "right" => bp.result_col = (bp.result_col + 1).min(bp.results.len().saturating_sub(1)),
                    "accept" => {
                        if let Some(id) = bp.results.get(bp.result_col).copied() {
                            bp.game = Some(id);
                            bp.view = 1;
                            bp.button = 0;
                            bp.ach_index = 0;
                        }
                    }
                    "backspace" => {
                        bp.query.pop();
                        search_changed = true;
                    }
                    a if a.starts_with("char:") => {
                        bp.query.push_str(&a[5..]);
                        bp.in_results = false;
                        search_changed = true;
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        if search_changed {
            self.bp_search_update();
        }
        if let Some(id) = favorite {
            self.toggle_favorite(id);
            self.rebuild_bp_rows();
        }
        self.refresh_bp();
        if let Some(id) = play {
            self.play(id);
        }
    }

    /// Événement manette : actif en Big Picture, ou pour y entrer (bouton Start / Guide).
    pub(crate) fn on_pad(self: &Rc<Self>, action: PadAction) {
        // Jamais pendant une partie lancée par Halcyon, ni quand la fenêtre n'a pas le focus.
        if self.active_session.get().is_some() || !self.window_focused() {
            return;
        }
        let active = self.bp.borrow().active;
        match (active, action) {
            (false, PadAction::Menu) => self.enter_big_picture(),
            (true, PadAction::Menu) => self.exit_big_picture(),
            (true, PadAction::Context) => self.bp_input("settings"),
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

fn bp_clamp(previous: &[usize], index: usize, len: usize) -> usize {
    previous.get(index).copied().unwrap_or(0).min(len.saturating_sub(1))
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
