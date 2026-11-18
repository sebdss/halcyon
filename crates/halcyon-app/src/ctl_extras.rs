//! Confort (1.1) : écran de lancement, rappel de pause, jeux offerts sur Epic, alerte de
//! disque presque plein et page Stockage.

use crate::controller::{post, Controller};
use crate::images::{ImageKey, Kind};
use crate::models::card_for;
use crate::{DetailUi, LaunchUi, ReqRow, StorageDisk, StorageGame, StorageUi};
use halcyon_core::{util, Game, GameId};
use halcyon_integrations::artwork::Downloader;
use halcyon_integrations::steam_requirements::{self as req, Level, Machine, Requirements};
use halcyon_integrations::{discord, epic_free, sysmon};
use halcyon_storage::now_unix;
use slint::{ComponentHandle, ModelRc, VecModel};
use std::rc::Rc;
use std::time::Duration;

/// Jeux non lancés depuis ce délai : proposés à la désinstallation.
const IDLE_SECS: i64 = 182 * 86_400;

/// Application Discord « Halcyon » (identifiant public, pas un secret ; logo : asset
/// `halcyon`). Un identifiant saisi dans les réglages la remplace.
const DISCORD_APP_ID: &str = "1572677279446995145";

/// Fiche de configuration gardée en cache.
const REQ_MAX_AGE: i64 = 30 * 86_400;

/// Configuration requise prête à afficher (calculée hors du fil de l'interface).
pub(crate) struct ReqData {
    req: Option<Requirements>,
    machine: Machine,
    free_mb: Option<u64>,
    offline: bool,
}

impl Controller {
    // ------------------------------------------------------------------ lancement

    /// Après le lancement d'un jeu : écran plein cadre quelques secondes, puis réduction de
    /// la fenêtre si ce réglage est actif.
    pub(crate) fn after_launch(self: &Rc<Self>, g: &Game) {
        self.start_break_reminder(g);
        self.update_presence();
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

impl Controller {
    // ------------------------------------------------------------------ Discord

    /// Statut Discord d'après la partie en cours (effacé sans partie ou si désactivé).
    pub(crate) fn update_presence(&self) {
        let (enabled, id) = {
            let s = self.settings.borrow();
            let id = if s.discord_client_id.is_empty() { DISCORD_APP_ID.to_string() } else { s.discord_client_id.clone() };
            (s.discord_presence, id)
        };
        let game = self.active_session.get().and_then(|id| self.lib.borrow().get(id).cloned());
        let mut presence = self.presence.borrow_mut();
        match game {
            Some(g) if enabled && !id.is_empty() => presence.set(
                &id,
                discord::Activity {
                    game: g.name.clone(),
                    state: g.platform.clone().unwrap_or_else(|| g.source.label()),
                    started: g.last_played.unwrap_or_else(now_unix),
                    image: g.artwork.cover_url.clone(),
                },
            ),
            _ => presence.clear(),
        }
    }

    // ------------------------------------------------------------------ bannière animée

    /// Images de la bannière animée : la bannière, puis trois captures en grand. Vide (bannière
    /// fixe) si l'option est coupée, en mode Performance ou s'il n'y a pas assez d'images.
    pub(crate) fn banner_slides(&self, g: &Game) -> Vec<slint::Image> {
        let (enabled, online) = {
            let s = self.settings.borrow();
            (s.animated_banner, s.online_metadata)
        };
        if !enabled || self.performance_active() || g.screenshots.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        if let Some(b) = self.images.get(ImageKey { game: g.id, kind: Kind::Banner }, g.artwork.banner.as_deref()) {
            out.push(b);
        }
        for url in g.screenshots.iter().take(3) {
            let full = full_screenshot(url);
            let Some(path) = self.remote.ensure(&full, online && self.net_allowed()) else { continue };
            if let Some(img) = self.images.get(ImageKey::remote(&full, Kind::Slide), Some(&path)) {
                out.push(img);
            }
        }
        if out.len() < 2 {
            out.clear();
        }
        out
    }

    // ------------------------------------------------------------------ configuration requise

    /// Configuration requise du jeu affiché (boutique Steam), comparée à ce PC.
    pub(crate) fn refresh_requirements(&self, g: &Game) {
        if self.req_shown.get() == Some(g.id) {
            return;
        }
        self.req_shown.set(Some(g.id));
        let Some(app) = self.app() else { return };
        let dui = app.global::<DetailUi>();
        dui.set_req_rows(ModelRc::default());
        dui.set_req_verdict("".into());
        dui.set_req_note("".into());
        dui.set_req_level(0);
        dui.set_req_status("".into());
        let Some(appid) = steam_appid(g) else { return };
        let online = self.net_allowed();
        let cache = self.paths.cache_dir.join("steam-req").join(format!("{appid}.json"));
        let (id, installed) = (g.id, g.installed);
        dui.set_req_status(tr!("Lecture de la configuration requise…").into());
        std::thread::Builder::new()
            .name("halcyon-req".into())
            .spawn(move || {
                let (value, offline) = requirements_json(&cache, appid, online);
                let req = value.and_then(|v| req::parse(&v, appid, cfg!(target_os = "linux")));
                let free_mb = (!installed).then(|| sysmon::list_disks().iter().filter(|d| !d.removable).map(|d| d.free_mb).max()).flatten();
                let data = ReqData { req, machine: machine(), free_mb, offline };
                post(move |c| c.show_requirements(id, installed, data));
            })
            .ok();
    }

    fn show_requirements(&self, id: GameId, installed: bool, data: ReqData) {
        if self.detail_id.get() != Some(id) {
            return;
        }
        let Some(app) = self.app() else { return };
        let dui = app.global::<DetailUi>();
        let Some(r) = data.req else {
            dui.set_req_status(if data.offline { tr!("Configuration requise indisponible hors ligne.") } else { "".into() }.into());
            return;
        };
        dui.set_req_status("".into());
        let m = &data.machine;
        let min = &r.minimum;
        let rec = r.recommended.as_ref();
        let size = |mb: Option<u64>| mb.map(|v| util::format_size(v * 1024 * 1024)).unwrap_or_default();
        let text = |t: &Option<String>| t.clone().unwrap_or_default();
        let mut rows: Vec<ReqRow> = Vec::new();
        let mut levels: Vec<(Level, String)> = Vec::new();
        let mut push = |label: String, minimum: String, recommended: String, yours: String, level: Level| {
            if minimum.is_empty() && recommended.is_empty() {
                return;
            }
            if level != Level::Unknown {
                levels.push((level, label.clone()));
            }
            rows.push(ReqRow { label: label.into(), minimum: minimum.into(), recommended: recommended.into(), yours: yours.into(), level: level as i32 });
        };
        let os_level = if (min.needs_64bit || rec.is_some_and(|r| r.needs_64bit)) && !m.is_64bit { Level::Below } else { Level::Unknown };
        push(tr!("Système"), text(&min.os), rec.map(|r| text(&r.os)).unwrap_or_default(), os_name(), os_level);
        push(tr!("Processeur"), text(&min.cpu), rec.map(|r| text(&r.cpu)).unwrap_or_default(), m.cpu.clone(), Level::Unknown);
        push(
            tr!("Mémoire"),
            size(min.ram_mb),
            rec.map(|r| size(r.ram_mb)).unwrap_or_default(),
            size(Some(m.ram_mb).filter(|&v| v > 0)),
            req::level(Some(m.ram_mb).filter(|&v| v > 0), min.ram_mb, rec.and_then(|r| r.ram_mb)),
        );
        let gpu = m.gpus.iter().max_by_key(|g| g.1.unwrap_or(0));
        let gpu_text = gpu.map(|(n, v)| match v {
            Some(v) => tr!("{name} ({vram})", name = n, vram = util::format_size(v * 1024 * 1024)),
            None => n.clone(),
        });
        push(
            tr!("Carte graphique"),
            text(&min.gpu),
            rec.map(|r| text(&r.gpu)).unwrap_or_default(),
            gpu_text.unwrap_or_default(),
            req::level(m.vram_mb(), min.vram_mb, rec.and_then(|r| r.vram_mb)),
        );
        let (disk_text, disk_level) = if installed {
            (tr!("Installé"), Level::Unknown)
        } else {
            let free = data.free_mb;
            (free.map(|f| tr!("{free} libres", free = util::format_size(f * 1024 * 1024))).unwrap_or_default(), req::level(free, min.storage_mb, rec.and_then(|r| r.storage_mb)))
        };
        push(tr!("Espace disque"), size(min.storage_mb), rec.map(|r| size(r.storage_mb)).unwrap_or_default(), disk_text, disk_level);

        let worst = levels.iter().map(|l| l.0).min().unwrap_or(Level::Unknown);
        let below: Vec<&str> = levels.iter().filter(|l| l.0 == Level::Below).map(|l| l.1.as_str()).collect();
        let verdict = match worst {
            Level::Unknown => tr!("Comparaison impossible : ce PC n'indique pas ces valeurs."),
            Level::Below => tr!("En dessous de la configuration minimale : {what}.", what = below.join(", ").to_lowercase()),
            Level::Minimum => tr!("Au niveau de la configuration minimale."),
            Level::Recommended if rec.is_some() => tr!("Au niveau de la configuration recommandée."),
            Level::Recommended => tr!("Au niveau de la configuration demandée."),
        };
        dui.set_req_verdict(verdict.into());
        dui.set_req_level(worst as i32);
        dui.set_req_note(tr!("Comparés : mémoire, mémoire vidéo, espace disque et 64 bits. Le processeur et la carte graphique s'affichent côte à côte : Halcyon ne mesure pas leurs performances.").into());
        dui.set_req_rows(ModelRc::new(VecModel::from(rows)));
    }
}

/// Numéro Steam d'un jeu : le sien, ou celui des visuels repris de la boutique.
pub(crate) fn steam_appid(g: &Game) -> Option<u64> {
    if g.source == halcyon_core::GameSource::Steam {
        return g.source_key.parse().ok();
    }
    let urls = [g.artwork.cover_url.as_deref(), g.artwork.banner_url.as_deref()].into_iter().flatten().chain(g.screenshots.iter().map(String::as_str));
    urls.filter_map(|u| {
        let rest = &u[u.find("/apps/")? + 6..];
        rest.split('/').next()?.parse().ok()
    })
    .next()
}

/// Adresse de la capture en 1920 × 1080 (la fiche garde les vignettes 600 × 338).
pub(crate) fn full_screenshot(url: &str) -> String {
    url.replace(".600x338.", ".1920x1080.")
}

/// Fiche `appdetails` : cache de 30 jours, sinon la boutique. Renvoie aussi « hors ligne ».
fn requirements_json(cache: &std::path::Path, appid: u64, online: bool) -> (Option<serde_json::Value>, bool) {
    let saved: Option<serde_json::Value> = std::fs::read_to_string(cache).ok().and_then(|t| serde_json::from_str(&t).ok());
    let fresh = saved.as_ref().and_then(|v| v.get("fetched")?.as_i64()).is_some_and(|t| now_unix() - t < REQ_MAX_AGE);
    let local = saved.as_ref().and_then(|v| v.get("store")).cloned();
    if fresh || !online {
        return (local.clone(), !online && local.is_none());
    }
    match Downloader::new().get_json(&req::url(appid)) {
        Ok(v) => {
            if let Some(dir) = cache.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(cache, serde_json::json!({ "fetched": now_unix(), "store": v }).to_string());
            (Some(v), false)
        }
        Err(e) => {
            log::info!("configuration requise de {appid} indisponible : {e}");
            (local, false)
        }
    }
}

/// Mesure de ce PC, faite une fois (cartes graphiques comprises).
fn machine() -> Machine {
    static MACHINE: std::sync::OnceLock<Machine> = std::sync::OnceLock::new();
    MACHINE.get_or_init(Machine::current).clone()
}

fn os_name() -> String {
    let name = sysinfo::System::long_os_version().unwrap_or_else(|_| std::env::consts::OS.to_string());
    if cfg!(target_pointer_width = "64") {
        tr!("{os}, 64 bits", os = name)
    } else {
        name
    }
}
