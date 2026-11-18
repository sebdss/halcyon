//! Gestionnaire de téléchargements (1.0.0) : file, pause et reprise, limite de débit,
//! progression et vitesse des téléchargements faits par Halcyon (Minecraft, mises à jour
//! de Halcyon). Les jeux des boutiques restent téléchargés par leurs clients officiels :
//! la page indique leur état et renvoie vers les mises à jour des jeux.

use crate::controller::Controller;
use crate::{DownloadsUi, TaskRow};
use halcyon_core::transfer;
use halcyon_core::UpdateState;
use slint::{ComponentHandle, ModelRc, VecModel};
use std::time::{Duration, Instant};

/// Tâches qui passent par le gestionnaire.
pub const KINDS: &[&str] = &["minecraft", "update"];

pub(crate) fn fmt_speed(bps: u64) -> String {
    if bps >= 1024 * 1024 {
        tr!("{v} Mio/s", v = halcyon_i18n::decimal(bps as f64 / (1024.0 * 1024.0), 1))
    } else {
        tr!("{v} Kio/s", v = halcyon_i18n::number(bps / 1024))
    }
}

impl Controller {
    /// Réglages enregistrés appliqués au contrôle commun (au démarrage et à chaque changement).
    pub(crate) fn apply_download_settings(&self) {
        let s = self.settings.borrow();
        let t = transfer::global();
        t.set_rate(s.download_limit_kib.saturating_mul(1024));
        t.set_max_concurrent(s.download_concurrency.max(1) as usize);
    }

    pub(crate) fn downloads_open(&self) {
        self.publish_tasks();
        self.downloads_refresh();
        // Vitesse : relevé toutes les secondes, seulement page affichée.
        let mut prev = (Instant::now(), transfer::global().received());
        let timer = slint::Timer::default();
        let weak = self.app.clone();
        timer.start(slint::TimerMode::Repeated, Duration::from_secs(1), move || {
            let Some(app) = weak.upgrade() else { return };
            let now = (Instant::now(), transfer::global().received());
            let ui = app.global::<DownloadsUi>();
            let t = transfer::global();
            ui.set_speed(if t.is_paused() { tr!("En pause").into() } else if t.active() == 0 { "—".into() } else { fmt_speed(transfer::speed(prev, now)).into() });
            ui.set_received(halcyon_core::util::format_size(now.1).into());
            ui.set_running(t.active() as i32);
            prev = now;
        });
        *self.downloads_timer.borrow_mut() = Some(timer);
    }

    pub(crate) fn downloads_close(&self) {
        self.downloads_timer.borrow_mut().take();
    }

    pub(crate) fn downloads_refresh(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<DownloadsUi>();
        let s = self.settings.borrow();
        let t = transfer::global();
        ui.set_paused(t.is_paused());
        ui.set_limit_kib(s.download_limit_kib.min(i32::MAX as u64) as i32);
        ui.set_concurrency(s.download_concurrency as i32);
        let lib = self.lib.borrow();
        let pending = lib.games.iter().filter(|g| g.update.state.pending() && !g.hidden);
        let (mut n, mut dl) = (0, 0);
        for g in pending {
            n += 1;
            if g.update.state == UpdateState::Downloading {
                dl += 1;
            }
        }
        ui.set_platform_pending(n);
        ui.set_platform_downloading(dl);
    }

    /// Lignes de tâches de téléchargement (appelé par `publish_tasks`).
    pub(crate) fn publish_download_rows(&self, active: &[TaskRow], history: &[TaskRow]) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<DownloadsUi>();
        let keep = |r: &&TaskRow| KINDS.contains(&r.kind.as_str());
        ui.set_active(ModelRc::new(VecModel::from(active.iter().filter(keep).cloned().collect::<Vec<_>>())));
        ui.set_history(ModelRc::new(VecModel::from(history.iter().filter(keep).take(20).cloned().collect::<Vec<_>>())));
    }

    pub(crate) fn set_downloads_paused(&self, on: bool) {
        transfer::global().set_paused(on);
        self.toast(if on { tr!("Téléchargements en pause") } else { tr!("Téléchargements repris") }, false);
        self.downloads_refresh();
    }

    pub(crate) fn set_download_limit(&self, kib: i32) {
        self.settings.borrow_mut().download_limit_kib = kib.max(0) as u64;
        self.save_settings();
        self.apply_download_settings();
        self.downloads_refresh();
    }

    pub(crate) fn set_download_concurrency(&self, n: i32) {
        self.settings.borrow_mut().download_concurrency = n.clamp(1, 8) as u32;
        self.save_settings();
        self.apply_download_settings();
        self.downloads_refresh();
    }
}
