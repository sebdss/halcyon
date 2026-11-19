//! Accueil 0.4 : tableau de bord de widgets, et menu contextuel commun des jeux.
//!
//! Seuls les widgets affichés sont calculés : un widget masqué ne coûte ni requête ni
//! lecture de disque. Les traitements lents (disques, captures) partent sur un thread.

use crate::controller::{post, Controller};
use crate::images::{ImageKey, Kind};
use crate::models::{card_for, detail_for, Tracking};
use crate::{
    ActivityRow, BarItem, CaptureItem, DetailUi, GameCard, GameMenu, HomeUi, KeyValue, LibraryStats, MenuEntry, WidgetRow,
    WidgetSlot,
};
use halcyon_core::model::{Game, GameId, GameSource, PlayStatus, PlaytimeOrigin};
use halcyon_core::util;
use halcyon_integrations::{launcher, steam};
use halcyon_storage::settings::{widget, Appearance, WidgetConf};
use halcyon_storage::now_unix;
use slint::{ComponentHandle, Image, ModelRc, SharedString, VecModel};
use std::path::PathBuf;
use std::rc::Rc;

/// Titres des widgets, dans l'ordre des codes de `settings::widget`.
pub const WIDGET_TITLES: [&str; widget::COUNT as usize] = [
    tr_noop!("Continuer à jouer"),
    tr_noop!("Derniers jeux lancés"),
    tr_noop!("Favoris"),
    tr_noop!("Bibliothèque"),
    tr_noop!("Succès récents"),
    tr_noop!("Statistiques"),
    tr_noop!("Temps de jeu"),
    tr_noop!("Activité récente"),
    tr_noop!("Stockage"),
    tr_noop!("Émulateurs"),
    tr_noop!("Captures"),
    tr_noop!("Recommandations"),
    tr_noop!("Ajoutés récemment"),
    tr_noop!("En cours"),
];

/// Nombre de jeux par étagère.
const SHELF: usize = 14;
/// Onglets de la fiche (voir `ui/pages/game.slint`).
pub mod tab {
    pub const ACHIEVEMENTS: i32 = 2;
    pub const SAVES: i32 = 5;
}

/// Disposition calculée : rangées de widgets ; `hero_first` : la première rangée est le
/// bandeau « Continuer à jouer », affiché en pleine largeur sous la barre supérieure.
#[derive(Debug, PartialEq)]
pub struct Layout {
    pub hero_first: bool,
    /// (code, colonnes occupées) ; chaque rangée totalise 3 colonnes.
    pub rows: Vec<Vec<(u8, u8)>>,
}

/// Range les widgets actifs dans des rangées de 3 colonnes, dans l'ordre configuré. Un
/// widget qui ne tient pas dans la place restante ouvre une nouvelle rangée ; le dernier
/// widget d'une rangée incomplète s'élargit pour la remplir (pas de trou). En mode
/// minimaliste : favoris et derniers jeux seulement.
pub fn layout(widgets: &[WidgetConf], minimal: bool) -> Layout {
    let active: Vec<(u8, u8)> = if minimal {
        vec![(widget::RECENT, 3), (widget::FAVORITES, 3)]
    } else {
        widgets.iter().filter(|w| w.enabled && w.kind < widget::COUNT).map(|w| (w.kind, w.span.clamp(1, 3))).collect()
    };
    let hero_first = active.first().is_some_and(|w| w.0 == widget::CONTINUE);
    let mut rows: Vec<Vec<(u8, u8)>> = Vec::new();
    let mut cur: Vec<(u8, u8)> = Vec::new();
    let mut used = 0u8;
    let close = |cur: &mut Vec<(u8, u8)>, used: u8, rows: &mut Vec<Vec<(u8, u8)>>| {
        if let Some(last) = cur.last_mut() {
            last.1 += 3 - used;
            rows.push(std::mem::take(cur));
        }
    };
    for &(kind, span) in &active {
        if used + span > 3 {
            close(&mut cur, used, &mut rows);
            used = 0;
        }
        cur.push((kind, span));
        used += span;
        if used == 3 {
            rows.push(std::mem::take(&mut cur));
            used = 0;
        }
    }
    close(&mut cur, used, &mut rows);
    Layout { hero_first, rows }
}

fn fmt_bytes(b: u64) -> String {
    const GB: f64 = 1_000_000_000.0;
    let g = b as f64 / GB;
    if g >= 1000.0 {
        tr!("{v} To", v = halcyon_i18n::decimal(g / 1000.0, 1))
    } else if g >= 10.0 {
        tr!("{v} Go", v = halcyon_i18n::number(g.round() as u64))
    } else {
        tr!("{v} Go", v = halcyon_i18n::decimal(g, 1))
    }
}

/// Disque monté et occupation.
#[derive(Clone, Debug)]
pub struct DiskUse {
    pub label: String,
    pub used: u64,
    pub total: u64,
}

/// Disques hébergeant au moins un jeu installé (ou les données de Halcyon).
fn disks_for(paths: &[PathBuf]) -> Vec<DiskUse> {
    // Espace seulement (pas de statistiques d'entrées-sorties).
    let Ok(disks) = sysinfo::Disks::new_with_refreshed_list_specifics(sysinfo::DiskRefreshKind::nothing().with_storage()) else {
        return Vec::new();
    };
    let mut out: Vec<(PathBuf, DiskUse)> = Vec::new();
    for p in paths {
        // Point de montage le plus long qui contient le chemin.
        let best = disks
            .iter()
            .filter(|d| p.starts_with(d.mount_point()))
            .max_by_key(|d| d.mount_point().as_os_str().len());
        let Some(d) = best else { continue };
        let mp = d.mount_point().to_path_buf();
        if out.iter().any(|(m, _)| *m == mp) || d.total_space() == 0 {
            continue;
        }
        let label = mp.display().to_string();
        let total = d.total_space();
        out.push((mp, DiskUse { label, used: total.saturating_sub(d.available_space()), total }));
    }
    out.into_iter().map(|(_, d)| d).collect()
}

impl Controller {
    fn home_widgets(&self) -> (Vec<WidgetConf>, bool) {
        let s = self.settings.borrow();
        (s.appearance.widgets(), s.appearance.ui_mode == 1)
    }

    /// Recalcule l'accueil : disposition, puis données des seuls widgets affichés.
    pub(crate) fn refresh_home(&self) {
        let Some(app) = self.app() else { return };
        let mut sw = crate::perf::Stopwatch::new("accueil");
        let (confs, minimal) = self.home_widgets();
        let lay = layout(&confs, minimal);
        let shown: Vec<u8> = lay.rows.iter().flatten().map(|w| w.0).collect();
        let on = |k: u8| shown.contains(&k);
        let home = app.global::<HomeUi>();
        let title = |k: u8| SharedString::from(halcyon_i18n::t(WIDGET_TITLES.get(usize::from(k)).copied().unwrap_or("?")));
        let rows: Vec<WidgetRow> = lay
            .rows
            .iter()
            .map(|r| WidgetRow {
                slots: ModelRc::new(VecModel::from(
                    r.iter().map(|&(k, s)| WidgetSlot { kind: i32::from(k), span: i32::from(s), title: title(k) }).collect::<Vec<_>>(),
                )),
            })
            .collect();
        let hidden_slots: Vec<WidgetSlot> = confs
            .iter()
            .filter(|w| !w.enabled)
            .map(|w| WidgetSlot { kind: i32::from(w.kind), span: i32::from(w.span), title: title(w.kind) })
            .collect();
        let hidden: Vec<WidgetRow> = hidden_slots
            .chunks(6)
            .map(|c| WidgetRow { slots: ModelRc::new(VecModel::from(c.to_vec())) })
            .collect();
        home.set_minimal(minimal);
        home.set_hero_first(lay.hero_first);
        home.set_widget_rows(ModelRc::new(VecModel::from(rows)));
        home.set_hidden_widgets(ModelRc::new(VecModel::from(hidden)));

        let now = now_unix();
        let lib = self.lib.borrow();
        let visible: Vec<&Game> = lib.games.iter().filter(|g| !g.hidden).collect();
        let count = |s: &[GameSource]| visible.iter().filter(|g| s.contains(&g.source)).count() as i32;
        // Toujours calculé (bon marché) : l'accueil vide en dépend.
        home.set_stats(LibraryStats {
            total: visible.len() as i32,
            installed: visible.iter().filter(|g| g.installed).count() as i32,
            steam: count(&[GameSource::Steam]),
            epic: count(&[GameSource::Epic]),
            others: count(&[GameSource::Standalone, GameSource::Manual, GameSource::Emulated, GameSource::Minecraft]),
            playtime: util::format_playtime(visible.iter().map(|g| g.effective_playtime()).sum()).into(),
        });

        let mut recent: Vec<&Game> = Vec::new();
        if on(widget::RECENT) || on(widget::CONTINUE) {
            recent = visible.iter().copied().filter(|g| g.last_played.is_some()).collect();
            recent.sort_by_key(|g| std::cmp::Reverse(g.last_played));
        }
        let mut added: Vec<&Game> = Vec::new();
        if on(widget::ADDED) || on(widget::CONTINUE) {
            added = visible.clone();
            added.sort_by_key(|g| std::cmp::Reverse((g.added_at, g.id)));
        }
        let cards = |list: &[&Game]| -> Vec<GameCard> { list.iter().take(SHELF).map(|g| card_for(g, &self.images, now)).collect() };
        if on(widget::RECENT) {
            self.shelves[0].set_vec(cards(&recent));
        }
        if on(widget::FAVORITES) {
            let mut fav: Vec<&Game> = visible.iter().copied().filter(|g| g.favorite).collect();
            fav.sort_by(|a, b| a.sort_key.cmp(&b.sort_key));
            self.shelves[1].set_vec(cards(&fav));
        }
        if on(widget::ADDED) {
            self.shelves[2].set_vec(cards(&added));
        }
        if on(widget::PLAYING) {
            let mut playing: Vec<&Game> = visible.iter().copied().filter(|g| g.status == PlayStatus::Playing).collect();
            playing.sort_by_key(|g| std::cmp::Reverse(g.last_played));
            home.set_playing(ModelRc::new(VecModel::from(cards(&playing))));
        }
        if on(widget::CONTINUE) {
            let hero = recent
                .iter()
                .find(|g| g.installed)
                .or_else(|| added.iter().find(|g| g.installed && g.artwork.banner.is_some()))
                .or_else(|| added.iter().find(|g| g.installed))
                .copied();
            self.hero_id.set(hero.map(|g| g.id));
            home.set_has_hero(hero.is_some());
            if let Some(g) = hero {
                home.set_hero(detail_for(g, &self.images, Tracking::default(), now));
                home.set_hero_kicker(if g.last_played.is_some() { tr!("Reprendre") } else { tr!("À découvrir") }.into());
                home.set_hero_meta(self.meta_line(g, now).into());
            }
        } else {
            self.hero_id.set(None);
            home.set_has_hero(false);
        }
        if on(widget::LIBRARY) {
            let total = visible.len().max(1) as f32;
            let rows: Vec<KeyValue> = GameSource::ALL
                .iter()
                .map(|s| (s, visible.iter().filter(|g| g.source == *s).count()))
                .filter(|(_, n)| *n > 0)
                .map(|(s, n)| KeyValue { label: s.label().into(), value: n.to_string().into(), ratio: n as f32 / total })
                .collect();
            home.set_library_summary(ModelRc::new(VecModel::from(rows)));
        }
        if on(widget::PLAYTIME) {
            let mut top: Vec<&Game> = visible.iter().copied().filter(|g| g.effective_playtime() > 0).collect();
            top.sort_by_key(|g| std::cmp::Reverse(g.effective_playtime()));
            let max = top.first().map_or(1, |g| g.effective_playtime()).max(1) as f32;
            let rows: Vec<KeyValue> = top
                .iter()
                .take(5)
                .map(|g| {
                    let origin = match g.playtime_origin() {
                        PlaytimeOrigin::Platform => g.source.label(),
                        PlaytimeOrigin::Manual => tr!("saisi"),
                        _ => tr!("suivi local"),
                    };
                    KeyValue {
                        label: g.name.as_str().into(),
                        value: format!("{} · {origin}", util::format_playtime(g.effective_playtime())).into(),
                        ratio: g.effective_playtime() as f32 / max,
                    }
                })
                .collect();
            home.set_playtime_summary(ModelRc::new(VecModel::from(rows)));
        }
        let install_dirs: Vec<PathBuf> =
            if on(widget::STORAGE) { visible.iter().filter(|g| g.installed).filter_map(|g| g.install_dir.clone()).collect() } else { Vec::new() };
        let declared: u64 = if on(widget::STORAGE) { visible.iter().filter(|g| g.installed).filter_map(|g| g.size_bytes).sum() } else { 0 };
        let with_size = visible.iter().filter(|g| g.installed && g.size_bytes.is_some()).count();
        drop(visible);
        drop(lib);

        sw.lap("bibliothèque et étagères");
        if on(widget::ACHIEVEMENTS) {
            self.publish_home_achievements();
            sw.lap("succès");
        }
        if on(widget::STATS) {
            self.publish_week();
            sw.lap("semaine");
        }
        if on(widget::ACTIVITY) {
            self.publish_home_activity();
            sw.lap("activité");
        }
        if on(widget::EMULATORS) {
            self.publish_home_emulators();
        }
        if on(widget::SUGGESTIONS) {
            self.publish_home_suggestions();
            sw.lap("recommandations");
        }
        if on(widget::STORAGE) {
            self.load_storage(install_dirs, declared, with_size);
        }
        if on(widget::CAPTURES) {
            self.load_captures();
        }
        sw.finish();
    }

    fn publish_home_achievements(&self) {
        let Some(app) = self.app() else { return };
        let list = self.db().recent_unlocks(5).unwrap_or_default();
        let rows: Vec<_> = list
            .iter()
            .map(|a| {
                let name = self.lib.borrow().get(a.game_id).map(|g| g.name.clone()).unwrap_or_default();
                self.achievement_row(a, &name)
            })
            .collect();
        app.global::<HomeUi>().set_recent_achievements(ModelRc::new(VecModel::from(rows)));
    }

    /// Sept derniers jours, d'après les sessions mesurées par Halcyon (requête bornée).
    fn publish_week(&self) {
        let Some(app) = self.app() else { return };
        let tz = crate::ctl_bigpicture::local_offset_secs();
        let now = now_unix();
        let today = (now + tz).div_euclid(86_400);
        let since = (today - 6) * 86_400 - tz;
        let sessions = self.db().sessions_since(since).unwrap_or_default();
        let mut days = [0u64; 7];
        for s in &sessions {
            let d = (s.started_at + tz).div_euclid(86_400) - (today - 6);
            if (0..7).contains(&d) {
                days[d as usize] += s.duration_secs;
            }
        }
        const NAMES: [&str; 7] = [trc_noop!("weekday", "jeu"), trc_noop!("weekday", "ven"), trc_noop!("weekday", "sam"), trc_noop!("weekday", "dim"), trc_noop!("weekday", "lun"), trc_noop!("weekday", "mar"), trc_noop!("weekday", "mer")]; // 1970-01-01 : jeudi
        let max = days.iter().copied().max().unwrap_or(0).max(1) as f32;
        let bars: Vec<BarItem> = days
            .iter()
            .enumerate()
            .map(|(i, &secs)| {
                let day = today - 6 + i as i64;
                BarItem {
                    label: if i == 6 { trc!("weekday", "auj.").into() } else { halcyon_i18n::tc("weekday", NAMES[day.rem_euclid(7) as usize]).into() },
                    value: if secs > 0 { util::format_playtime(secs).into() } else { SharedString::new() },
                    ratio: secs as f32 / max * f32::from(u8::from(secs > 0)),
                }
            })
            .collect();
        let home = app.global::<HomeUi>();
        let total: u64 = days.iter().sum();
        home.set_week_total(if total > 0 { util::format_playtime(total) } else { tr!("0 min") }.into());
        home.set_week_bars(ModelRc::new(VecModel::from(bars)));
    }

    fn publish_home_activity(&self) {
        let Some(app) = self.app() else { return };
        let entries = self.db().activities(&[], "", 6).unwrap_or_default();
        let now = now_unix();
        let tz = crate::ctl_bigpicture::local_offset_secs();
        let rows: Vec<ActivityRow> = entries
            .iter()
            .map(|a| {
                let s = (a.at + tz).rem_euclid(86_400);
                let same_day = (a.at + tz).div_euclid(86_400) == (now + tz).div_euclid(86_400);
                ActivityRow {
                    id: a.id as i32,
                    game_id: a.game_id.unwrap_or(0) as i32,
                    category: a.kind.category() as i32,
                    title: a.title.as_str().into(),
                    detail: a.detail.as_str().into(),
                    day: SharedString::new(),
                    time: if same_day {
                        format!("{:02}:{:02}", s / 3600, s / 60 % 60)
                    } else {
                        util::format_relative(a.at + tz, now + tz)
                    }
                    .into(),
                    new_day: false,
                }
            })
            .collect();
        app.global::<HomeUi>().set_recent_activity(ModelRc::new(VecModel::from(rows)));
    }

    fn publish_home_emulators(&self) {
        let Some(app) = self.app() else { return };
        let rows: Vec<KeyValue> = {
            let reg = self.registry.borrow();
            let detected = self.detected.borrow();
            let lib = self.lib.borrow();
            reg.emulators
                .iter()
                .filter(|d| detected.contains_key(&d.id))
                .map(|def| {
                    let n = lib
                        .games
                        .iter()
                        .filter(|g| g.source == GameSource::Emulated && g.platform.as_ref().is_some_and(|p| def.platforms.contains_key(p)))
                        .count();
                    KeyValue {
                        label: def.name.as_str().into(),
                        value: if n == 0 { tr!("détecté").into() } else { trn!(n, "{n} jeu", "{n} jeux").into() },
                        ratio: 0.0,
                    }
                })
                .collect()
        };
        app.global::<HomeUi>().set_emulator_summary(ModelRc::new(VecModel::from(rows)));
    }

    fn publish_home_suggestions(&self) {
        let Some(app) = self.app() else { return };
        let s = self.suggestions(0, 4);
        let mut ids: Vec<GameId> = Vec::new();
        for g in s.resume.iter().chain(&s.almost_done).chain(&s.discover).chain(&s.rediscover) {
            if !ids.contains(&g.game) {
                ids.push(g.game);
            }
        }
        let now = now_unix();
        let cards: Vec<GameCard> = {
            let lib = self.lib.borrow();
            ids.iter().filter_map(|id| lib.get(*id)).take(SHELF).map(|g| card_for(g, &self.images, now)).collect()
        };
        app.global::<HomeUi>().set_suggested(ModelRc::new(VecModel::from(cards)));
    }

    /// Occupation des disques : lue sur un thread (un montage réseau peut être lent).
    fn load_storage(&self, mut dirs: Vec<PathBuf>, declared: u64, with_size: usize) {
        dirs.push(self.paths.data_dir.clone());
        std::thread::Builder::new()
            .name("halcyon-disks".into())
            .spawn(move || {
                let disks = disks_for(&dirs);
                post(move |c| {
                    let Some(app) = c.app() else { return };
                    let mut rows: Vec<KeyValue> = disks
                        .iter()
                        .map(|d| KeyValue {
                            label: d.label.as_str().into(),
                            value: tr!("{used} libres sur {total}", used = fmt_bytes(d.total.saturating_sub(d.used)), total = fmt_bytes(d.total)).into(),
                            ratio: d.used as f32 / d.total.max(1) as f32,
                        })
                        .collect();
                    if declared > 0 {
                        rows.push(KeyValue {
                            label: tr!("Jeux installés ({with_size} avec taille connue)", with_size = with_size).into(),
                            value: fmt_bytes(declared).into(),
                            ratio: disks.iter().map(|d| d.total).max().map_or(0.0, |t| declared as f32 / t.max(1) as f32).min(1.0),
                        });
                    }
                    app.global::<HomeUi>().set_storage(ModelRc::new(VecModel::from(rows)));
                });
            })
            .ok();
    }

    /// Captures Steam locales (dossier `userdata`), lues sur un thread.
    fn load_captures(&self) {
        let root = steam::SteamScanner::new(self.settings.borrow().steam_path.clone()).root();
        let Some(root) = root else {
            self.home_captures.borrow_mut().clear();
            if let Some(app) = self.app() {
                let home = app.global::<HomeUi>();
                home.set_captures_note(tr!("Aucune source de captures : Steam n'est pas installé ou introuvable.").into());
                home.set_captures(ModelRc::default());
            }
            return;
        };
        std::thread::Builder::new()
            .name("halcyon-captures".into())
            .spawn(move || {
                let shots = steam::screenshots(&root, None, 12);
                post(move |c| {
                    *c.home_captures.borrow_mut() = shots;
                    c.publish_captures();
                });
            })
            .ok();
    }

    /// Republie les captures (au fil des décodages de vignettes).
    pub(crate) fn publish_captures(&self) {
        let Some(app) = self.app() else { return };
        let shots = self.home_captures.borrow().clone();
        let items: Vec<CaptureItem> = {
            let lib = self.lib.borrow();
            shots
                .iter()
                .map(|s| {
                    let game = lib.games.iter().find(|g| g.source == GameSource::Steam && g.source_key == s.appid.to_string());
                    let src = s.thumbnail.as_ref().unwrap_or(&s.path);
                    let key = ImageKey::remote(&src.display().to_string(), Kind::Shot);
                    CaptureItem {
                        image: self.images.get(key, Some(src)).unwrap_or_default(),
                        game_id: game.map_or(-1, |g| g.id as i32),
                        title: game.map_or_else(|| format!("App {}", s.appid), |g| g.name.clone()).into(),
                        date: util::format_date(s.modified).into(),
                        path: s.path.display().to_string().into(),
                    }
                })
                .collect()
        };
        let home = app.global::<HomeUi>();
        home.set_captures_note(tr!("Aucune capture Steam trouvée. Les captures prises avec F12 dans Steam apparaîtront ici.").into());
        home.set_captures(ModelRc::new(VecModel::from(items)));
    }

    /// Captures Steam du jeu affiché : lues une fois par fiche, sur un thread.
    pub(crate) fn load_detail_captures(&self, g: &Game) {
        let cached = self.detail_captures.borrow().0 == Some(g.id);
        if cached {
            self.publish_detail_captures();
            return;
        }
        *self.detail_captures.borrow_mut() = (Some(g.id), Vec::new());
        self.publish_detail_captures();
        let appid = if g.source == GameSource::Steam { g.source_key.parse::<u64>().ok() } else { None };
        let root = steam::SteamScanner::new(self.settings.borrow().steam_path.clone()).root();
        let (Some(appid), Some(root)) = (appid, root) else { return };
        let id = g.id;
        std::thread::Builder::new()
            .name("halcyon-captures".into())
            .spawn(move || {
                let shots = steam::screenshots(&root, Some(appid), 200);
                post(move |c| {
                    if c.detail_captures.borrow().0 == Some(id) {
                        c.detail_captures.borrow_mut().1 = shots;
                        c.publish_detail_captures();
                    }
                });
            })
            .ok();
    }

    pub(crate) fn publish_detail_captures(&self) {
        let Some(app) = self.app() else { return };
        let shots = self.detail_captures.borrow().1.clone();
        let items: Vec<CaptureItem> = shots
            .iter()
            .map(|s| {
                let src = s.thumbnail.as_ref().unwrap_or(&s.path);
                let key = ImageKey::remote(&src.display().to_string(), Kind::Shot);
                CaptureItem {
                    image: self.images.get(key, Some(src)).unwrap_or_default(),
                    game_id: -1,
                    title: SharedString::new(),
                    date: util::format_date(s.modified).into(),
                    path: s.path.display().to_string().into(),
                }
            })
            .collect();
        app.global::<DetailUi>().set_captures(ModelRc::new(VecModel::from(items)));
    }

    fn open_detail_capture(&self, index: usize) {
        let shot = self.detail_captures.borrow().1.get(index).cloned();
        if let Some(s) = shot {
            if let Err(e) = launcher::open_path(&s.path) {
                self.toast(tr!("Ouverture impossible : {e}", e = e), true);
            }
        }
    }

    fn open_capture(&self, index: usize) {
        let shot = self.home_captures.borrow().get(index).cloned();
        if let Some(s) = shot {
            if let Err(e) = launcher::open_path(&s.path) {
                self.toast(tr!("Ouverture impossible : {e}", e = e), true);
            }
        }
    }

    // ------------------------------------------------------------------ édition des widgets

    fn edit_widgets(&self, f: impl FnOnce(&mut Vec<WidgetConf>)) {
        {
            let mut s = self.settings.borrow_mut();
            let mut w = s.appearance.widgets();
            f(&mut w);
            s.appearance.widgets = w;
        }
        self.save_settings();
        self.refresh_home();
        self.refresh_appearance_ui();
    }

    /// Déplace un widget avant/après le widget actif voisin.
    pub(crate) fn widget_move(&self, kind: u8, delta: i32) {
        self.edit_widgets(|w| {
            let Some(i) = w.iter().position(|x| x.kind == kind) else { return };
            let mut j = i as i64;
            loop {
                j += i64::from(delta.signum());
                if j < 0 || j as usize >= w.len() {
                    return;
                }
                if w[j as usize].enabled {
                    break;
                }
            }
            let item = w.remove(i);
            w.insert(j as usize, item);
        });
    }

    pub(crate) fn widget_resize(&self, kind: u8, span: u8) {
        self.edit_widgets(|w| {
            if let Some(x) = w.iter_mut().find(|x| x.kind == kind) {
                x.span = span.clamp(1, 3);
            }
        });
    }

    pub(crate) fn widget_toggle(&self, kind: u8, on: bool) {
        self.edit_widgets(|w| {
            if let Some(i) = w.iter().position(|x| x.kind == kind) {
                let mut x = w.remove(i);
                x.enabled = on;
                // Réactivé : placé à la fin des widgets affichés.
                let at = if on { w.iter().rposition(|y| y.enabled).map_or(0, |p| p + 1) } else { w.len() };
                w.insert(at, x);
            }
        });
    }

    pub(crate) fn widgets_reset(&self) {
        self.edit_widgets(|w| *w = Appearance::default_widgets());
        self.toast(tr!("Disposition de l'accueil réinitialisée"), false);
    }

    // ------------------------------------------------------------------ menu contextuel

    /// Ouvre le menu d'un jeu à la position donnée (coordonnées de la fenêtre ; négatives :
    /// centre de la fenêtre, pour la touche Menu de la manette).
    pub(crate) fn open_game_menu(&self, id: GameId, x: f32, y: f32) {
        let Some(app) = self.app() else { return };
        let game = { self.lib.borrow().get(id).cloned() };
        let Some(g) = game else { return };
        let mut entries: Vec<MenuEntry> = Vec::new();
        let mut codes: Vec<SharedString> = Vec::new();
        let mut add = |code: &str, label: &str, icon: Image, enabled: bool, danger: bool, sep: bool| {
            entries.push(MenuEntry { label: halcyon_i18n::t(label).into(), icon, enabled, danger, separator_before: sep });
            codes.push(code.into());
        };
        let icons = app.global::<crate::Icons>();
        if g.installed {
            add("play", tr_noop!("Jouer"), icons.get_play(), true, false, false);
        } else {
            add("play", tr_noop!("Non installé"), icons.get_play(), false, false, false);
        }
        add("open", tr_noop!("Ouvrir la fiche"), icons.get_info(), true, false, false);
        add("fav", if g.favorite { tr_noop!("Retirer des favoris") } else { tr_noop!("Ajouter aux favoris") }, icons.get_star(), true, false, true);
        // Jeux cochés dans la bibliothèque : l'action porte sur toute la sélection.
        let marked = { self.lib.borrow().marked.len() };
        if marked > 1 && self.lib.borrow().marked.contains(&id) {
            add("collect", tr_noop!("Ajouter la sélection à une collection…"), icons.get_layers(), true, false, false);
        } else {
            add("collect", tr_noop!("Ajouter à une collection…"), icons.get_layers(), true, false, false);
        }
        add("achievements", tr_noop!("Succès"), icons.get_trophy(), self.ach_mode(&g) != "unsupported", false, false);
        add("saves", tr_noop!("Sauvegardes"), icons.get_save(), true, false, false);
        let has_dir = g.install_dir.as_ref().is_some_and(|d| d.is_dir());
        add("folder", tr_noop!("Ouvrir le dossier du jeu"), icons.get_folder(), has_dir, false, true);
        add("shortcut", tr_noop!("Créer un raccourci sur le bureau"), icons.get_external(), g.installed, false, false);
        add("hide", if g.hidden { tr_noop!("Afficher dans la bibliothèque") } else { tr_noop!("Masquer de la bibliothèque") }, icons.get_eye_off(), true, false, true);
        if crate::ctl_uninstall::can_uninstall(&g) && g.source != GameSource::Manual {
            let label = if g.source == GameSource::Emulated { tr_noop!("Masquer la ROM…") } else { tr_noop!("Désinstaller…") };
            add("uninstall", label, icons.get_trash(), true, true, false);
        }
        if g.source == GameSource::Manual {
            add("remove", tr_noop!("Retirer de Halcyon…"), icons.get_trash(), true, true, false);
        }
        let ui = app.global::<GameMenu>();
        let size = app.window().size().to_logical(app.window().scale_factor());
        ui.set_game_id(id as i32);
        ui.set_title(g.name.clone().into());
        ui.set_entries(ModelRc::new(VecModel::from(entries)));
        ui.set_codes(ModelRc::new(VecModel::from(codes)));
        ui.set_selected(0);
        ui.set_x(if x < 0.0 { size.width / 2.0 - 130.0 } else { x });
        ui.set_y(if y < 0.0 { size.height / 2.0 - 170.0 } else { y });
        ui.set_open(true);
    }

    pub(crate) fn open_game_tab(&self, id: GameId, tab: i32) {
        self.open_game(id);
        if let Some(app) = self.app() {
            app.global::<DetailUi>().set_tab(tab);
            app.global::<DetailUi>().invoke_tools_refresh();
        }
    }

    pub(crate) fn on_game_menu(self: &Rc<Self>, code: &str) {
        let Some(app) = self.app() else { return };
        let id = GameId::from(app.global::<GameMenu>().get_game_id());
        match code {
            "play" => self.play(id),
            "open" => self.open_game(id),
            "fav" => self.toggle_favorite(id),
            "collect" => self.open_collection_picker(id),
            "achievements" => self.open_game_tab(id, tab::ACHIEVEMENTS),
            "saves" => self.open_game_tab(id, tab::SAVES),
            "folder" => {
                let dir = self.lib.borrow().get(id).and_then(|g| g.install_dir.clone());
                if let Some(d) = dir {
                    self.open_folder(&d.display().to_string());
                }
            }
            "shortcut" => self.create_game_shortcut(id),
            "uninstall" => self.uninstall_request(id),
            "hide" => self.toggle_hidden(id),
            "remove" => {
                let name = self.lib.borrow().get(id).map(|g| g.name.clone()).unwrap_or_default();
                self.confirm(
                    &tr!("Retirer ce jeu ?"),
                    &tr!("« {name} » sera retiré de Halcyon. Les fichiers du jeu ne sont pas supprimés.", name = name),
                    &tr!("Retirer"),
                    move |c| c.remove_game(id),
                );
            }
            _ => {}
        }
    }

    pub(crate) fn wire_home(self: &Rc<Self>, app: &crate::AppWindow) {
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
        let home = app.global::<HomeUi>();
        home.on_widget_move(with!(|c, k, d| c.widget_move(k.clamp(0, 255) as u8, d)));
        home.on_widget_resize(with!(|c, k, s| c.widget_resize(k.clamp(0, 255) as u8, s.clamp(1, 3) as u8)));
        home.on_widget_toggle(with!(|c, k, on| c.widget_toggle(k.clamp(0, 255) as u8, on)));
        home.on_widgets_reset(with!(|c| c.widgets_reset()));
        home.on_open_capture(with!(|c, i| c.open_capture(i.max(0) as usize)));
        app.global::<DetailUi>().on_open_capture(with!(|c, i| c.open_detail_capture(i.max(0) as usize)));
        let menu = app.global::<GameMenu>();
        menu.on_request(with!(|c, id, x, y| c.open_game_menu(GameId::from(id), x, y)));
        menu.on_chosen(with!(|c, code| c.on_game_menu(&code)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(kind: u8, span: u8, enabled: bool) -> WidgetConf {
        WidgetConf { kind, span, enabled }
    }

    #[test]
    fn layout_packs_rows_of_three() {
        use widget::*;
        let l = layout(&[w(CONTINUE, 3, true), w(RECENT, 3, true), w(PLAYTIME, 1, true), w(STATS, 2, true), w(LIBRARY, 2, true), w(STORAGE, 2, true)], false);
        assert!(l.hero_first);
        assert_eq!(l.rows, vec![vec![(CONTINUE, 3)], vec![(RECENT, 3)], vec![(PLAYTIME, 1), (STATS, 2)], vec![(LIBRARY, 3)], vec![(STORAGE, 3)]]);
        for r in &l.rows {
            assert_eq!(r.iter().map(|x| x.1).sum::<u8>(), 3);
        }
    }

    #[test]
    fn disabled_widgets_are_skipped_and_hero_only_first() {
        use widget::*;
        let l = layout(&[w(CONTINUE, 3, false), w(LIBRARY, 1, true), w(CONTINUE, 3, true)], false);
        assert!(!l.hero_first);
        assert_eq!(l.rows, vec![vec![(LIBRARY, 3)], vec![(CONTINUE, 3)]]);
        assert_eq!(layout(&[], false).rows.len(), 0);
    }

    #[test]
    fn minimal_mode_shows_recent_and_favorites() {
        let l = layout(&Appearance::default_widgets(), true);
        assert!(!l.hero_first);
        assert_eq!(l.rows, vec![vec![(widget::RECENT, 3)], vec![(widget::FAVORITES, 3)]]);
    }

    #[test]
    fn bytes_are_formatted() {
        assert_eq!(fmt_bytes(1_500_000_000), "1,5 Go");
        assert_eq!(fmt_bytes(512_000_000_000), "512 Go");
        assert_eq!(fmt_bytes(2_000_000_000_000), "2,0 To");
    }
}
