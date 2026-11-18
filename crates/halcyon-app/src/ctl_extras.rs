//! Confort (1.1) : écran de lancement, rappel de pause, jeux offerts sur Epic, alerte de
//! disque presque plein et page Stockage.

use crate::controller::{post, Controller};
use crate::images::{ImageKey, Kind};
use crate::models::card_for;
use crate::{LaunchUi, StorageDisk, StorageGame, StorageUi};
use halcyon_core::{util, Game, GameId};
use halcyon_integrations::artwork::Downloader;
use halcyon_integrations::{epic_free, sysmon};
use halcyon_storage::now_unix;
use slint::{ComponentHandle, ModelRc, VecModel};
use std::rc::Rc;
use std::time::Duration;

/// Jeux non lancés depuis ce délai : proposés à la désinstallation.
const IDLE_SECS: i64 = 182 * 86_400;

impl Controller {
    // ------------------------------------------------------------------ lancement

    /// Après le lancement d'un jeu : écran plein cadre quelques secondes, puis réduction de
    /// la fenêtre si ce réglage est actif.
    pub(crate) fn after_launch(self: &Rc<Self>, g: &Game) {
        self.start_break_reminder(g);
        if !self.settings.borrow().launch_screen {
            self.minimize_if_enabled();
            return;
        }
        let Some(app) = self.app() else { return };
        let ui = app.global::<LaunchUi>();
        let img = |kind, path: &Option<std::path::PathBuf>| self.images.get(ImageKey { game: g.id, kind }, path.as_deref());
        let banner = img(Kind::Banner, &g.artwork.banner).or_else(|| img(Kind::Cover, &g.artwork.cover));
        let logo = img(Kind::Logo, &g.artwork.logo);
        ui.set_title(g.name.as_str().into());
        ui.set_has_banner(banner.is_some());
        ui.set_banner(banner.unwrap_or_default());
        ui.set_has_logo(logo.is_some());
        ui.set_logo(logo.unwrap_or_default());
        ui.set_card(card_for(g, &self.images, now_unix()));
        ui.set_running(false);
        ui.set_open(true);
        let weak = Rc::downgrade(self);
        let id = g.id;
        // « Bonne partie » une fois le jeu démarré (ou après quelques secondes sans signal).
        slint::Timer::single_shot(Duration::from_millis(3200), move || {
            let Some(c) = weak.upgrade() else { return };
            c.launch_screen_running(id);
        });
    }

    /// Le jeu tourne : message final, puis fermeture de l'écran.
    pub(crate) fn launch_screen_running(self: &Rc<Self>, id: GameId) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<LaunchUi>();
        if !ui.get_open() || ui.get_running() || self.active_session.get().is_some_and(|a| a != id) {
            return;
        }
        ui.set_running(true);
        let weak = Rc::downgrade(self);
        slint::Timer::single_shot(Duration::from_millis(1800), move || {
            let Some(c) = weak.upgrade() else { return };
            if let Some(app) = c.app() {
                app.global::<LaunchUi>().set_open(false);
            }
            c.minimize_if_enabled();
        });
    }

    // ------------------------------------------------------------------ rappel de pause

    fn start_break_reminder(self: &Rc<Self>, g: &Game) {
        let hours = self.settings.borrow().break_reminder_hours;
        if hours == 0 {
            *self.break_timer.borrow_mut() = None;
            return;
        }
        let weak = Rc::downgrade(self);
        let (id, name) = (g.id, g.name.clone());
        let started = now_unix();
        let timer = slint::Timer::default();
        timer.start(slint::TimerMode::Repeated, Duration::from_secs(u64::from(hours) * 3600), move || {
            let Some(c) = weak.upgrade() else { return };
            if c.active_session.get() != Some(id) {
                *c.break_timer.borrow_mut() = None;
                return;
            }
            let msg = tr!("Vous jouez à {game} depuis {time}. Une petite pause ?", game = name, time = util::format_playtime((now_unix() - started).max(0) as u64));
            crate::notify::info("Halcyon", &msg);
            c.toast(msg, false);
        });
        *self.break_timer.borrow_mut() = Some(timer);
    }

    pub(crate) fn stop_break_reminder(&self) {
        *self.break_timer.borrow_mut() = None;
    }

    // ------------------------------------------------------------------ jeux offerts

    /// Signale les jeux offerts en ce moment sur l'Epic Games Store (une fois chacun).
    pub(crate) fn check_free_games(&self) {
        if !self.settings.borrow().free_games_notifications || !self.net_allowed() {
            return;
        }
        let locale = halcyon_i18n::language();
        let country = locale.split(['-', '_']).nth(1).unwrap_or("FR").to_uppercase();
        let url = epic_free::url(&locale, &country);
        std::thread::Builder::new()
            .name("halcyon-epic-free".into())
            .spawn(move || {
                let games = match Downloader::new().get_json(&url) {
                    Ok(v) => epic_free::parse(&v, &locale),
                    Err(e) => return log::info!("jeux offerts Epic indisponibles : {e}"),
                };
                post(move |c| c.on_free_games(games));
            })
            .ok();
    }

    fn on_free_games(&self, games: Vec<epic_free::FreeGame>) {
        let fresh: Vec<&epic_free::FreeGame> = {
            let s = self.settings.borrow();
            games.iter().filter(|g| !s.free_games_seen.contains(&g.id)).collect()
        };
        if fresh.is_empty() {
            return;
        }
        {
            let mut s = self.settings.borrow_mut();
            s.free_games_seen.extend(fresh.iter().map(|g| g.id.clone()));
            // Les offres passées ne servent plus : on garde les plus récentes.
            let n = s.free_games_seen.len();
            if n > 60 {
                s.free_games_seen.drain(..n - 60);
            }
        }
        self.save_settings();
        let names: Vec<&str> = fresh.iter().map(|g| g.title.as_str()).collect();
        let until = fresh.iter().filter_map(|g| g.until).min();
        let msg = match until {
            Some(t) => tr!("Offert sur l'Epic Games Store jusqu'au {date} : {games}", date = util::format_date(t), games = names.join(", ")),
            None => tr!("Offert sur l'Epic Games Store : {games}", games = names.join(", ")),
        };
        log::info!("{msg}");
        crate::notify::info("Halcyon", &msg);
        self.toast(msg, false);
        *self.free_games.borrow_mut() = games;
        self.refresh_storage_free_games();
    }

    fn refresh_storage_free_games(&self) {
        let Some(app) = self.app() else { return };
        let rows: Vec<StorageGame> = self
            .free_games
            .borrow()
            .iter()
            .map(|g| StorageGame {
                id: -1,
                title: g.title.as_str().into(),
                detail: g.until.map(|t| tr!("jusqu'au {date}", date = util::format_date(t))).unwrap_or_default().into(),
                size: "".into(),
                url: g.url.as_str().into(),
                card: Default::default(),
            })
            .collect();
        app.global::<StorageUi>().set_free_games(ModelRc::new(VecModel::from(rows)));
    }

    // ------------------------------------------------------------------ disques

    /// Prévient (au plus une fois par jour) qu'un disque est presque plein.
    pub(crate) fn check_disks(&self) {
        let today = now_unix() / 86_400;
        {
            let s = self.settings.borrow();
            if !s.disk_alerts || s.disk_alert_day == today {
                return;
            }
        }
        std::thread::spawn(move || {
            let low: Vec<sysmon::DiskStat> = sysmon::list_disks().into_iter().filter(sysmon::disk_low).collect();
            if low.is_empty() {
                return;
            }
            post(move |c| {
                c.settings.borrow_mut().disk_alert_day = today;
                c.save_settings();
                let list: Vec<String> = low.iter().map(|d| tr!("{mount} ({free} libres)", mount = d.mount, free = util::format_size(d.free_mb * 1024 * 1024))).collect();
                let msg = tr!("Disque presque plein : {list}. Voyez Outils › Stockage pour faire de la place.", list = list.join(", "));
                crate::notify::info("Halcyon", &msg);
                c.toast(msg, true);
            });
        });
    }

    /// Page Stockage : disques, et jeux installés non lancés depuis six mois (les plus gros
    /// d'abord). La désinstallation passe par la plateforme du jeu.
    pub(crate) fn refresh_storage(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<StorageUi>();
        let now = now_unix();
        let lib = self.lib.borrow();
        let list: Vec<sysmon::DiskStat> = sysmon::list_disks().into_iter().filter(|d| !d.removable).collect();
        // Disques montés les uns dans les autres (« / » et « /home ») : chaque jeu compte une
        // fois, sur le point de montage le plus long qui contient son dossier.
        let mut per_disk = vec![0u64; list.len()];
        for g in lib.games.iter().filter(|g| g.installed) {
            let (Some(dir), Some(size)) = (g.install_dir.as_deref(), g.size_bytes) else { continue };
            let best = list.iter().enumerate().filter(|(_, d)| dir.starts_with(&d.mount)).max_by_key(|(_, d)| d.mount.len());
            if let Some((i, _)) = best {
                per_disk[i] += size;
            }
        }
        let disks: Vec<StorageDisk> = list
            .iter()
            .zip(&per_disk)
            .map(|(d, &games)| StorageDisk {
                mount: d.mount.as_str().into(),
                detail: tr!(
                    "{free} libres sur {total} · jeux de la bibliothèque : {games}",
                    free = util::format_size(d.free_mb * 1024 * 1024),
                    total = util::format_size(d.total_mb * 1024 * 1024),
                    games = util::format_size(games)
                )
                .into(),
                used: 1.0 - d.free_mb as f32 / d.total_mb.max(1) as f32,
                low: sysmon::disk_low(d),
            })
            .collect();
        ui.set_disks(ModelRc::new(VecModel::from(disks)));
        let mut idle: Vec<&Game> = lib
            .games
            .iter()
            .filter(|g| g.installed && !g.hidden && g.size_bytes.unwrap_or(0) > 0)
            .filter(|g| g.last_played.or(g.installed_at).is_none_or(|t| now - t > IDLE_SECS))
            .collect();
        idle.sort_by_key(|g| std::cmp::Reverse(g.size_bytes.unwrap_or(0)));
        let reclaim: u64 = idle.iter().filter_map(|g| g.size_bytes).sum();
        ui.set_reclaim(util::format_size(reclaim).into());
        let rows: Vec<StorageGame> = idle
            .iter()
            .take(40)
            .map(|g| StorageGame {
                id: g.id as i32,
                title: g.name.as_str().into(),
                detail: match g.last_played {
                    Some(t) => tr!("{source} · dernière partie {when}", source = g.source.label(), when = util::format_relative_inline(t, now)),
                    None => tr!("{source} · jamais lancé", source = g.source.label()),
                }
                .into(),
                size: util::format_size(g.size_bytes.unwrap_or(0)).into(),
                url: "".into(),
                card: card_for(g, &self.images, now),
            })
            .collect();
        ui.set_idle(ModelRc::new(VecModel::from(rows)));
        drop(lib);
        self.refresh_storage_free_games();
    }
}
