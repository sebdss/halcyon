//! Contrôleur — succès : synchronisation (Steam, RetroAchievements), surveillance
//! pendant une partie, notifications, statistiques et succès personnalisés.

use crate::controller::{post, Controller};
use crate::images::{ImageKey, Kind};
use crate::{AchUi, AchievementRow, AppUi, DetailUi, MonthBar, ProviderStat};
use halcyon_core::achievements::{rarity_label, Achievement, AchievementData, AchievementProvider, SyncStatus};
use halcyon_core::util;
use halcyon_core::{Game, GameId, GameSource};
use halcyon_integrations::artwork::{Downloader, FetchError};
use halcyon_integrations::rahash;
use halcyon_integrations::retro::RetroAchievements;
use halcyon_integrations::steam_web::{self, SteamWeb};
use halcyon_integrations::{steam, steam_stats};
use halcyon_storage::{now_unix, Database};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Une synchronisation complète ne réinterroge pas un jeu synchronisé depuis moins de 12 h
/// (sauf s'il a été lancé depuis).
const RESYNC_SECS: i64 = 12 * 3600;
const POLL_MAX: Duration = Duration::from_secs(8 * 3600);
/// Surveillance par la lecture locale : un fichier relu, pas de requête.
const POLL_LOCAL: Duration = Duration::from_secs(20);

/// Jeu à synchroniser (copie autonome, envoyée au thread de travail).
#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    id: GameId,
    name: String,
    source: GameSource,
    key: String,
    platform: Option<String>,
    rom: Option<PathBuf>,
    ra_id: Option<u32>,
}

/// Paramètres des fournisseurs, copiés pour le thread de travail.
#[derive(Clone)]
pub(crate) struct SyncConfig {
    steam: Option<SteamWeb>,
    /// Succès Steam lus dans le cache du client (sans clé).
    steam_local: Option<SteamLocal>,
    /// Réseau autorisé (sinon : lecture locale seulement).
    online: bool,
    /// Rareté publique demandée à Steam (pas pendant la surveillance d'une partie).
    rarity: bool,
    ra: Option<RetroAchievements>,
    /// plateforme → (console RetroAchievements, méthode d'empreinte)
    platforms: HashMap<String, (u32, String)>,
    cache_dir: PathBuf,
    remote_dir: PathBuf,
}

/// Client Steam de ce PC et compte connecté : de quoi lire les succès sur le disque.
#[derive(Clone)]
pub(crate) struct SteamLocal {
    root: PathBuf,
    account: u32,
}

/// Succès nouvellement débloqué, prêt à être notifié.
pub(crate) struct Unlock {
    achievement: Achievement,
    game: String,
    icon: Option<PathBuf>,
}

impl Controller {
    pub(crate) fn steam_web(&self) -> Option<SteamWeb> {
        let s = self.settings.borrow();
        let key = s.steam_api_key.trim().to_string();
        if key.is_empty() {
            return None;
        }
        let id = s.steam_id.trim().parse::<u64>().ok();
        drop(s);
        let steam_id = id.or_else(|| self.local_steam_id())?;
        Some(SteamWeb { key, steam_id })
    }

    /// Lecture locale des succès Steam : client Steam trouvé et compte connu (réglage, sinon
    /// dernier compte connecté sur ce PC).
    pub(crate) fn steam_local(&self) -> Option<SteamLocal> {
        let s = self.settings.borrow();
        if !s.steam_enabled {
            return None;
        }
        let id = s.steam_id.trim().parse::<u64>().ok();
        let root = steam::SteamScanner::new(s.steam_path.clone()).root()?;
        drop(s);
        let id = id.or_else(|| steam::local_steam_id64(&root))?;
        root.join("appcache").join("stats").is_dir().then(|| SteamLocal { root, account: steam_stats::account_id(id) })
    }

    fn retro(&self) -> Option<RetroAchievements> {
        let s = self.settings.borrow();
        (!s.ra_user.trim().is_empty() && !s.ra_api_key.trim().is_empty())
            .then(|| RetroAchievements { user: s.ra_user.trim().into(), key: s.ra_api_key.trim().into() })
    }

    fn sync_config(&self) -> SyncConfig {
        let platforms = self
            .registry
            .borrow()
            .platforms
            .iter()
            .filter_map(|p| p.ra_console.map(|c| (p.id.clone(), (c, p.ra_hash.clone()))))
            .collect();
        SyncConfig {
            steam: self.steam_web(),
            steam_local: self.steam_local(),
            online: self.net_allowed(),
            rarity: true,
            ra: self.retro(),
            platforms,
            cache_dir: self.paths.cache_dir.join("retroachievements"),
            remote_dir: self.paths.cache_dir.join("remote"),
        }
    }

    /// Mode d'affichage de l'onglet Succès pour un jeu.
    pub(crate) fn ach_mode(&self, g: &Game) -> &'static str {
        match g.source {
            GameSource::Steam if self.steam_web().is_some() || self.steam_local().is_some() => "steam",
            GameSource::Steam => "unconfigured",
            GameSource::Epic => "epic",
            GameSource::Emulated => {
                let supported = g
                    .platform
                    .as_deref()
                    .and_then(|p| self.registry.borrow().platform(p).map(|p| p.ra_console.is_some()))
                    .unwrap_or(false);
                match (supported, self.retro().is_some()) {
                    (false, _) => "unsupported",
                    (true, true) => "retroachievements",
                    (true, false) => "unconfigured",
                }
            }
            // GOG : pas d'API publique sans connexion au compte ; succès personnalisés seulement.
            GameSource::Standalone | GameSource::Manual | GameSource::Minecraft | GameSource::Gog => "custom",
        }
    }

    fn candidate(g: &Game) -> Candidate {
        Candidate {
            id: g.id,
            name: g.name.clone(),
            source: g.source,
            key: g.source_key.clone(),
            platform: g.platform.clone(),
            rom: g.executable.clone(),
            ra_id: g.ra_game_id,
        }
    }

    pub(crate) fn reload_progress(&self) {
        let p = self.db().progress_by_game().unwrap_or_default();
        *self.progress.borrow_mut() = p;
    }

    // ------------------------------------------------------------------ synchronisation

    /// Synchronise un jeu (`only`) ou toute la bibliothèque (incrémental sauf `force`).
    pub(crate) fn start_ach_sync(&self, only: Option<GameId>, force: bool) {
        let cfg = self.sync_config();
        // Hors ligne, seuls les succès Steam lus sur le disque restent disponibles.
        if !cfg.online && cfg.steam_local.is_none() {
            if only.is_some() && force {
                self.toast(tr!("Hors ligne : les succès seront synchronisés au retour de la connexion"), false);
            }
            return;
        }
        if cfg.steam.is_none() && cfg.steam_local.is_none() && cfg.ra.is_none() {
            if only.is_some() {
                self.toast(tr!("Configurez vos comptes Steam / RetroAchievements dans les paramètres"), true);
            }
            return;
        }
        if self.ach_syncing.replace(true) {
            if only.is_some() {
                self.toast(tr!("Une synchronisation est déjà en cours"), false);
            }
            return;
        }
        let now = now_unix();
        let candidates: Vec<Candidate> = {
            let db = self.db();
            let steam_times = db.sync_times(AchievementProvider::Steam).unwrap_or_default();
            let ra_times = db.sync_times(AchievementProvider::RetroAchievements).unwrap_or_default();
            drop(db);
            let lib = self.lib.borrow();
            let mut list: Vec<&Game> = lib
                .games
                .iter()
                .filter(|g| only.is_none_or(|id| g.id == id))
                .filter(|g| match g.source {
                    GameSource::Steam => cfg.steam_local.is_some() || (cfg.steam.is_some() && cfg.online),
                    GameSource::Emulated => {
                        cfg.online && cfg.ra.is_some() && g.platform.as_ref().is_some_and(|p| cfg.platforms.contains_key(p)) && g.installed
                    }
                    _ => false,
                })
                .filter(|g| {
                    if force || only.is_some() {
                        return true;
                    }
                    let times = if g.source == GameSource::Steam { &steam_times } else { &ra_times };
                    // Progression locale réécrite par Steam depuis la dernière lecture.
                    let local_changed = |t: i64| {
                        g.source == GameSource::Steam
                            && cfg.steam_local.as_ref().is_some_and(|l| {
                                g.source_key.parse().ok().and_then(|appid| steam_stats::progress_modified(&l.root, l.account, appid)).is_some_and(|m| m > t)
                            })
                    };
                    match times.get(&g.id) {
                        None => g.installed || g.playtime_secs > 0,
                        Some(&t) => now - t > RESYNC_SECS || g.last_played.is_some_and(|lp| lp > t) || local_changed(t),
                    }
                })
                .collect();
            // Les jeux joués récemment d'abord.
            list.sort_by_key(|g| std::cmp::Reverse(g.last_played.unwrap_or(0)));
            list.into_iter().map(Self::candidate).collect()
        };
        if candidates.is_empty() {
            self.ach_syncing.set(false);
            if only.is_some() {
                self.toast(tr!("Rien à synchroniser pour ce jeu"), false);
            }
            return;
        }
        self.set_sync_ui(true, &trn!(candidates.len(), "Synchronisation de {n} jeu…", "Synchronisation de {n} jeux…"));
        let db = self.db.clone();
        std::thread::Builder::new()
            .name("halcyon-achievements".into())
            .spawn(move || {
                let dl = Downloader::new();
                let total = candidates.len();
                let mut unlocks = Vec::new();
                let mut errors = Vec::new();
                for (i, c) in candidates.iter().enumerate() {
                    if total > 1 && i % 5 == 0 {
                        let msg = tr!("Succès : {v}/{total}", v = i + 1, total = total);
                        post(move |ctl| ctl.set_sync_ui(true, &msg));
                    }
                    match sync_one(c, &cfg, &dl, &db) {
                        Ok(mut u) => unlocks.append(&mut u),
                        Err(FetchError::Network(e)) => {
                            errors.push(tr!("réseau : {e}", e = e));
                            break;
                        }
                        Err(FetchError::Unavailable(e)) => errors.push(format!("{} : {e}", c.name)),
                    }
                    // Pauses entre les requêtes réseau (la lecture locale n'en a pas besoin).
                    if cfg.online {
                        std::thread::sleep(Duration::from_millis(if c.source == GameSource::Steam { 250 } else { 450 }));
                    }
                }
                let games = db.lock().unwrap_or_else(|e| e.into_inner()).load_games();
                post(move |ctl| ctl.on_ach_sync_done(unlocks, errors, total, games.ok()));
            })
            .ok();
    }

    fn set_sync_ui(&self, syncing: bool, status: &str) {
        if syncing {
            let id = self.task_start("ach", &tr!("Synchronisation des succès"));
            self.task_update(id, None, Some(status));
        }
        if let Some(app) = self.app() {
            let ach = app.global::<AchUi>();
            ach.set_syncing(syncing);
            ach.set_status(status.into());
            app.global::<DetailUi>().set_ach_syncing(syncing);
        }
    }

    fn on_ach_sync_done(&self, unlocks: Vec<Unlock>, errors: Vec<String>, total: usize, games: Option<Vec<Game>>) {
        self.ach_syncing.set(false);
        if let Some(games) = games {
            self.reload_games(games);
            self.refresh_library();
        }
        self.reload_progress();
        let status = match errors.first() {
            Some(e) => tr!("Jeux traités : {total} · erreurs : {errors} — {e}", total = total, errors = errors.len(), e = e),
            None => trn!(total, "Synchronisé à {clock_now} · {n} jeu", "Synchronisé à {clock_now} · {n} jeux", clock_now = clock_now()),
        };
        let unlocked = unlocks.len();
        let detail = if unlocked > 0 { trn!(unlocked, "{status} · {n} nouveau succès", "{status} · {n} nouveaux succès", status = status) } else { status.clone() };
        let id = self.tasks.borrow().find_kind("ach");
        if let Some(id) = id {
            self.task_finish(id, errors.is_empty(), detail, crate::tasks::Action::OpenPage("achievements"));
        }
        for e in &errors {
            log::warn!("succès : {e}");
        }
        self.set_sync_ui(false, &status);
        self.refresh_ach_page();
        self.refresh_detail();
        if self.bp.borrow().active {
            self.refresh_bp();
        }
        self.notify_unlocks(unlocks);
    }

    // ------------------------------------------------------------------ surveillance pendant le jeu

    /// Vérifie périodiquement les succès du jeu lancé (s'il est pris en charge).
    pub(crate) fn start_polling(&self, id: GameId) {
        let generation = self.poll_generation.fetch_add(1, Ordering::SeqCst) + 1;
        if !self.settings.borrow().achievement_polling {
            return;
        }
        let Some(game) = self.lib.borrow().get(id).cloned() else { return };
        let mut cfg = self.sync_config();
        // La rareté est déjà connue ; la surveillance ne relit que la progression.
        cfg.rarity = false;
        let local = cfg.steam_local.is_some() && cfg.steam.is_none();
        if !cfg.online && !(game.source == GameSource::Steam && cfg.steam_local.is_some()) {
            return;
        }
        let interval = match self.ach_mode(&game) {
            "steam" if local || !cfg.online => POLL_LOCAL,
            "steam" => Duration::from_secs(120),
            "retroachievements" => Duration::from_secs(90),
            _ => return,
        };
        let cand = Self::candidate(&game);
        let db = self.db.clone();
        let current = self.poll_generation.clone();
        log::info!("surveillance des succès de « {} »", game.name);
        std::thread::Builder::new()
            .name("halcyon-ach-poll".into())
            .spawn(move || {
                let dl = Downloader::new();
                let started = Instant::now();
                'outer: while started.elapsed() < POLL_MAX {
                    let wait = Instant::now();
                    while wait.elapsed() < interval {
                        std::thread::sleep(Duration::from_secs(2));
                        if current.load(Ordering::SeqCst) != generation {
                            break 'outer;
                        }
                    }
                    match sync_one(&cand, &cfg, &dl, &db) {
                        Ok(unlocks) if !unlocks.is_empty() => post(move |c| {
                            c.reload_progress();
                            c.refresh_detail();
                            c.refresh_ach_page();
                            c.notify_unlocks(unlocks);
                        }),
                        Ok(_) => {}
                        Err(e) => log::debug!("surveillance : {e}"),
                    }
                }
                log::info!("fin de la surveillance des succès");
            })
            .ok();
    }

    pub(crate) fn stop_polling(&self) {
        self.poll_generation.fetch_add(1, Ordering::SeqCst);
    }

    fn notify_unlocks(&self, unlocks: Vec<Unlock>) {
        if unlocks.is_empty() {
            return;
        }
        self.sound(halcyon_audio::Cue::Major);
        let notify = self.settings.borrow().achievement_notifications;
        // Au-delà de trois, un résumé évite une avalanche de notifications.
        if unlocks.len() > 3 {
            if notify {
                crate::notify::achievement(
                    &trn!(unlocks.len(), "{n} succès débloqué", "{n} succès débloqués"),
                    "",
                    &unlocks[0].game,
                    None,
                    unlocks[0].icon.as_deref(),
                );
            }
        } else if notify {
            for u in &unlocks {
                let rarity = u.achievement.data.rarity.map(|r| format!("{} ({})", rarity_label(r), tr!("{v} %", v = halcyon_i18n::decimal(r as f64, 1))));
                crate::notify::achievement(
                    &u.achievement.data.name,
                    &u.achievement.data.description,
                    &u.game,
                    rarity.as_deref(),
                    u.icon.as_deref(),
                );
            }
        }
        // Notification dans l'interface (visible si Halcyon est affiché).
        if let (Some(app), Some(u)) = (self.app(), unlocks.last()) {
            let row = self.achievement_row(&u.achievement, &u.game);
            let ui = app.global::<AppUi>();
            ui.set_ach_toast(row);
            ui.set_ach_toast_visible(true);
        }
    }

    // ------------------------------------------------------------------ vues

    pub(crate) fn achievement_row(&self, a: &Achievement, game: &str) -> AchievementRow {
        let online = true;
        let icon = a.data.icon_url.as_ref().and_then(|url| {
            let path = self.remote.ensure(url, online && self.net_allowed())?;
            self.images.get(ImageKey::remote(url, Kind::Icon), Some(&path))
        });
        let d = &a.data;
        let date = match (d.unlocked, d.unlocked_at) {
            (true, Some(t)) => util::format_date(t),
            (true, None) => tr!("Débloqué").into(),
            _ => String::new(),
        };
        let progress = d.progress.filter(|(_, max)| *max > 0.0);
        AchievementRow {
            id: a.id as i32,
            game_id: a.game_id as i32,
            name: d.name.as_str().into(),
            description: d.description.as_str().into(),
            game: game.into(),
            has_icon: icon.is_some(),
            icon: icon.unwrap_or_default(),
            unlocked: d.unlocked,
            date: date.into(),
            rarity: d.rarity.map(|r| format!("{} · {}", rarity_label(r), tr!("{v} %", v = halcyon_i18n::decimal(r as f64, 1)))).unwrap_or_default().into(),
            rarity_pct: d.rarity.unwrap_or(100.0),
            has_rarity: d.rarity.is_some(),
            points: d.points.map(|p| trn!(p, "{n} point", "{n} points")).unwrap_or_default().into(),
            hardcore: d.unlocked_hardcore_at.is_some(),
            hidden: d.hidden,
            progress: progress.map(|(c, m)| (c / m) as f32).unwrap_or(0.0),
            has_progress: progress.is_some(),
            progress_text: progress.map(|(c, m)| format!("{c} / {m}")).unwrap_or_default().into(),
            custom: a.provider == AchievementProvider::Custom,
            provider: a.provider.label().into(),
        }
    }

    pub(crate) fn refresh_detail_achievements(&self, g: &Game) {
        let Some(app) = self.app() else { return };
        let dui = app.global::<DetailUi>();
        let list = self.db().load_achievements(g.id).unwrap_or_default();
        let rows: Vec<AchievementRow> = list.iter().map(|a| self.achievement_row(a, &g.name)).collect();
        let unlocked = list.iter().filter(|a| a.data.unlocked).count();
        let mode = self.ach_mode(g);
        let summary = match (mode, list.len()) {
            ("epic", _) => tr!("Epic Games ne fournit pas d'API publique de succès : ils ne peuvent pas être affichés ici."),
            ("unsupported", _) => tr!("Cette plateforme n'est pas prise en charge par RetroAchievements."),
            ("unconfigured", _) if g.source == GameSource::Steam => {
                tr!("Aucun compte Steam trouvé sur ce PC : ouvrez Steam une fois connecté, puis relancez une analyse.")
            }
            ("unconfigured", _) => tr!("Renseignez votre compte RetroAchievements dans les paramètres."),
            ("custom", 0) => tr!("Aucun succès personnalisé pour l'instant."),
            (_, 0) => tr!("Aucun succès synchronisé pour ce jeu."),
            (_, n) => trn!(n, "{unlocked} / {n} succès débloqué", "{unlocked} / {n} succès débloqués", unlocked = halcyon_i18n::number(unlocked as u64)),
        };
        let provider = match g.source {
            GameSource::Steam => Some(AchievementProvider::Steam),
            GameSource::Emulated => Some(AchievementProvider::RetroAchievements),
            _ => None,
        };
        let status = provider
            .and_then(|p| self.db().sync_status(g.id, p).ok().flatten())
            .map(|(t, s)| {
                let when = util::format_relative_inline(t, now_unix());
                match s {
                    SyncStatus::Ok => tr!("Synchronisé {when}", when = when),
                    SyncStatus::NoAchievements => tr!("Ce jeu n'a pas de succès sur cette plateforme (vérifié {when})", when = when),
                    SyncStatus::NotRecognized => {
                        tr!("ROM non reconnue par RetroAchievements : indiquez l'identifiant du jeu dans l'onglet Paramètres.").into()
                    }
                    SyncStatus::NotCached => tr!(
                        "Steam n'a pas encore enregistré les succès de ce jeu sur ce PC : lancez-le une fois. Pour les jeux jamais lancés ici, une clé Steam Web API (Paramètres › Comptes, réglages avancés) les récupère en ligne."
                    )
                    .into(),
                    SyncStatus::Error(e) => tr!("Dernière tentative {when} : {e}", when = when, e = e),
                }
            })
            .unwrap_or_else(|| if mode == "custom" { tr!("Succès locaux, non vérifiés par une plateforme.").into() } else { String::new() });
        dui.set_achievements(ModelRc::new(VecModel::from(rows)));
        dui.set_ach_summary(summary.into());
        dui.set_ach_status(status.into());
        dui.set_ach_mode(mode.into());
        dui.set_ach_percent(if list.is_empty() { -1.0 } else { unlocked as f32 / list.len() as f32 });
        dui.set_ach_syncing(self.ach_syncing.get());
    }

    pub(crate) fn refresh_ach_page(&self) {
        let Some(app) = self.app() else { return };
        let ach = app.global::<AchUi>();
        ach.set_steam_ready(self.steam_web().is_some() || self.steam_local().is_some());
        ach.set_ra_ready(self.retro().is_some());
        let stats = self.db().achievement_stats().unwrap_or_default();
        ach.set_unlocked(stats.unlocked as i32);
        ach.set_total(stats.total as i32);
        ach.set_percent(if stats.total == 0 { 0.0 } else { stats.unlocked as f32 / stats.total as f32 });
        ach.set_completed(stats.completed_games.len() as i32);
        ach.set_games(stats.games_with_achievements as i32);
        ach.set_points(stats.points as i32);
        let providers: Vec<ProviderStat> = stats
            .by_provider
            .iter()
            .map(|(p, prog)| ProviderStat {
                name: p.label().into(),
                unlocked: prog.unlocked as i32,
                total: prog.total as i32,
                percent: prog.percent() / 100.0,
            })
            .collect();
        ach.set_providers(ModelRc::new(VecModel::from(providers)));
        let max = stats.per_month.iter().map(|(_, n)| *n).max().unwrap_or(1).max(1);
        let months: Vec<MonthBar> = stats
            .per_month
            .iter()
            .map(|(m, n)| {
                let label = m
                    .split_once('-')
                    .and_then(|(y, mo)| Some(util::format_month(util::days_from_civil(y.parse().ok()?, mo.parse().ok()?, 1) * 86_400)))
                    .unwrap_or_else(|| m.clone());
                MonthBar { label: label.into(), count: *n as i32, ratio: *n as f32 / max as f32 }
            })
            .collect();
        ach.set_months(ModelRc::new(VecModel::from(months)));

        let names: HashMap<GameId, String> = self.lib.borrow().games.iter().map(|g| (g.id, g.name.clone())).collect();
        let to_rows = |list: Vec<Achievement>| -> Vec<AchievementRow> {
            list.iter().map(|a| self.achievement_row(a, names.get(&a.game_id).map(String::as_str).unwrap_or(""))).collect()
        };
        ach.set_recent(ModelRc::new(VecModel::from(to_rows(self.db().recent_unlocks(8).unwrap_or_default()))));
        ach.set_rarest(ModelRc::new(VecModel::from(to_rows(self.db().rarest_unlocks(5).unwrap_or_default()))));
        let now = now_unix();
        let completed: Vec<crate::GameCard> = {
            let lib = self.lib.borrow();
            stats.completed_games.iter().filter_map(|id| lib.get(*id)).map(|g| crate::models::card_for(g, &self.images, now)).collect()
        };
        ach.set_completed_games(ModelRc::new(VecModel::from(completed)));
    }

    // ------------------------------------------------------------------ actions

    pub(crate) fn add_custom_achievement(&self, game: GameId, name: &str, desc: &str) {
        let result = self.db().add_custom_achievement(game, name, desc);
        match result {
            Ok(()) => {
                self.reload_progress();
                self.refresh_detail();
                self.refresh_ach_page();
            }
            Err(e) => self.toast(e.to_string(), true),
        }
    }

    pub(crate) fn toggle_custom_achievement(&self, id: i64) {
        let Some(game) = self.detail_id.get() else { return };
        let current = self.db().load_achievements(game).unwrap_or_default().into_iter().find(|a| a.id == id);
        let Some(a) = current else { return };
        let unlocked = !a.data.unlocked;
        if self.db().set_custom_unlocked(id, unlocked).is_ok() {
            self.reload_progress();
            self.refresh_detail();
            self.refresh_ach_page();
        }
    }

    pub(crate) fn delete_custom_achievement(&self, id: i64) {
        if self.db().delete_custom_achievement(id).is_ok() {
            self.reload_progress();
            self.refresh_detail();
            self.refresh_ach_page();
        }
    }

    pub(crate) fn set_ra_id(&self, id: GameId, value: &str) {
        let v = value.trim();
        let parsed = if v.is_empty() { None } else { v.parse::<u32>().ok() };
        if !v.is_empty() && parsed.is_none() {
            self.toast(tr!("L'identifiant RetroAchievements est un nombre"), true);
            return;
        }
        if let Err(e) = self.db().set_ra_game_id(id, parsed) {
            self.toast(tr!("Erreur : {e}", e = e), true);
            return;
        }
        if let Some(g) = self.lib.borrow_mut().get_mut(id) {
            g.ra_game_id = parsed;
        }
        self.toast(tr!("Identifiant enregistré"), false);
        self.start_ach_sync(Some(id), true);
    }
}

fn clock_now() -> String {
    let s = now_unix().rem_euclid(86_400);
    format!("{:02}:{:02} UTC", s / 3600, s / 60 % 60)
}

/// Synchronise un jeu. Renvoie les succès nouvellement débloqués.
fn sync_one(c: &Candidate, cfg: &SyncConfig, dl: &Downloader, db: &Arc<Mutex<Database>>) -> Result<Vec<Unlock>, FetchError> {
    let lock = || db.lock().unwrap_or_else(|e| e.into_inner());
    let (provider, result) = match c.source {
        GameSource::Steam => {
            let Ok(appid) = c.key.parse::<u64>() else { return Ok(vec![]) };
            let local = || {
                cfg.steam_local.as_ref().map(|l| steam_stats::local_achievements(&l.root, l.account, appid, steam_web::steam_language()))
            };
            // Rareté publique (sans clé), si le réseau est permis.
            let with_rarity = |mut list: Vec<AchievementData>| {
                if cfg.online && cfg.rarity && !list.is_empty() {
                    if let Ok(rarity) = steam_web::global_rarity(dl, appid) {
                        for a in &mut list {
                            a.rarity = rarity.get(&a.api_name).copied();
                        }
                    }
                }
                list
            };
            let result = match (&cfg.steam, cfg.online) {
                // Clé Web API : source complète (tous les jeux du compte). En cas d'échec (clé
                // refusée, profil privé, réseau), la lecture locale prend le relais.
                (Some(web), true) => match web.achievements(dl, appid) {
                    Ok(list) => Ok(list.unwrap_or_default()),
                    Err(e) => match local() {
                        Some(Ok(Some(list))) => Ok(with_rarity(list)),
                        _ => Err(e),
                    },
                },
                _ => match local() {
                    None => return Ok(vec![]),
                    Some(Ok(None)) => {
                        let _ = lock().set_sync_status(c.id, AchievementProvider::Steam, &SyncStatus::NotCached);
                        return Ok(vec![]);
                    }
                    Some(Ok(Some(list))) => Ok(with_rarity(list)),
                    Some(Err(e)) => Err(FetchError::Unavailable(e)),
                },
            };
            (AchievementProvider::Steam, result)
        }
        GameSource::Emulated => {
            let Some(ra) = &cfg.ra else { return Ok(vec![]) };
            let Some((console, method)) = c.platform.as_ref().and_then(|p| cfg.platforms.get(p)) else { return Ok(vec![]) };
            let game_id = match c.ra_id {
                Some(id) => Some(id),
                None => {
                    let hash = c.rom.as_ref().and_then(|r| rahash::hash_rom(r, method).ok().flatten());
                    match hash {
                        Some(h) => {
                            let found = ra.hash_table(dl, *console, &cfg.cache_dir)?.get(&h).copied();
                            if let Some(id) = found {
                                let _ = lock().set_ra_game_id(c.id, Some(id));
                            }
                            found
                        }
                        None => None,
                    }
                }
            };
            let Some(game_id) = game_id else {
                let _ = lock().set_sync_status(c.id, AchievementProvider::RetroAchievements, &SyncStatus::NotRecognized);
                return Ok(vec![]);
            };
            let progress = ra.progress(dl, game_id);
            if let Ok(p) = &progress {
                if let Some(url) = &p.box_art {
                    let _ = lock().set_cover_url_if_missing(c.id, url);
                }
            }
            (AchievementProvider::RetroAchievements, progress.map(|p| p.achievements))
        }
        _ => return Ok(vec![]),
    };
    let list = match result {
        Ok(l) => l,
        Err(e) => {
            let _ = lock().set_sync_status(c.id, provider, &SyncStatus::Error(e.to_string()));
            return Err(e);
        }
    };
    let newly = lock()
        .merge_achievements(c.id, provider, &list)
        .map_err(|e| FetchError::Unavailable(tr!("base de données : {e}", e = e)))?;
    // Icônes des nouveaux succès téléchargées tout de suite (pour la notification).
    Ok(newly
        .into_iter()
        .map(|a| {
            let icon = a.data.icon_url.as_ref().and_then(|url| {
                let path = cfg.remote_dir.join(format!("{:016x}.img", util::stable_hash(url)));
                (path.is_file() || dl.fetch_to(url, &path).is_ok()).then_some(path)
            });
            Unlock { achievement: a, game: c.name.clone(), icon }
        })
        .collect())
}
