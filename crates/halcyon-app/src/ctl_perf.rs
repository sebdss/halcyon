//! Performance Center : mesures en direct, applications gourmandes, recommandations,
//! historique par partie et analyse des journaux de temps d'image.
//!
//! Coût maîtrisé :
//! - mesures en direct sur un fil dédié, **uniquement quand la page est affichée**
//!   (échantillon toutes les 1,5 s ; arrêt dès qu'on change de page) ;
//! - pendant une partie (si l'historique est activé) : un échantillon toutes les 10 s,
//!   sans liste des processus, résumé écrit en base à la fin ;
//! - rien d'autre ne tourne.

use crate::controller::{post, Controller};
use crate::{PerfAppRow, PerfDiskRow, PerfGpuRow, PerfRecoRow, PerfSessionRow, PerfUi};
use halcyon_core::model::GameId;
use halcyon_integrations::frametimes;
use halcyon_integrations::sysmon::{self, fmt_mb, History, Level, Monitor, RecoAction, RecoContext, Snapshot};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const LIVE_INTERVAL: Duration = Duration::from_millis(1500);
const SESSION_INTERVAL: Duration = Duration::from_secs(10);
/// Points des courbes (1,5 s chacun : 90 s affichées).
const CURVE_POINTS: usize = 60;

#[derive(Default)]
pub(crate) struct PerfState {
    live: Option<Worker>,
    session: Option<Worker>,
    history: History,
    samples: u64,
}

/// Fil de mesure : arrêté par son drapeau, réveillé pour s'arrêter sans attendre.
struct Worker {
    stop: Arc<AtomicBool>,
    thread: std::thread::Thread,
}

impl Worker {
    fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.unpark();
    }
}

fn spawn(name: &str, f: impl FnOnce(Arc<AtomicBool>) + Send + 'static) -> Option<Worker> {
    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    std::thread::Builder::new().name(name.into()).spawn(move || f(s)).ok().map(|h| Worker { stop, thread: h.thread().clone() })
}

fn pct(v: f32) -> SharedString {
    format!("{v:.0} %").into()
}

impl Controller {
    // ------------------------------------------------------------------ mesures en direct

    pub(crate) fn perf_open(&self) {
        self.refresh_perf_sessions();
        let mut st = self.perfc.borrow_mut();
        if st.live.is_some() {
            return;
        }
        st.history = History::new(CURVE_POINTS);
        st.samples = 0;
        st.live = spawn("perf-live", |stop| {
            let mut m = Monitor::new();
            let mut n: u64 = 0;
            while !stop.load(Ordering::Relaxed) {
                // Disques : une fois sur dix (leur taille change lentement).
                let snap = m.sample(true, n % 10 == 0);
                n += 1;
                post(move |c| c.perf_publish(snap));
                std::thread::park_timeout(LIVE_INTERVAL);
            }
        });
        drop(st);
        if let Some(app) = self.app() {
            app.global::<PerfUi>().set_sampling(true);
        }
    }

    /// Page quittée : plus aucune mesure en direct.
    pub(crate) fn perf_close(&self) {
        if let Some(w) = self.perfc.borrow_mut().live.take() {
            w.stop();
            if let Some(app) = self.app() {
                app.global::<PerfUi>().set_sampling(false);
            }
        }
    }

    fn perf_publish(&self, snap: Snapshot) {
        let Some(app) = self.app() else { return };
        // Échantillon arrivé après la fermeture de la page.
        if self.perfc.borrow().live.is_none() {
            return;
        }
        let ui = app.global::<PerfUi>();
        let (cpu_curve, gpu_curve, stable, keep_disks) = {
            let mut st = self.perfc.borrow_mut();
            st.history.push(&snap);
            st.samples += 1;
            let curve = |v: &[f32]| -> Vec<f32> { v.iter().map(|x| x / 100.0).collect() };
            (curve(st.history.cpu()), curve(st.history.gpu()), st.samples >= 5, snap.disks.is_empty())
        };
        ui.set_cpu_name(if snap.cpu_name.is_empty() { "Processeur".into() } else { snap.cpu_name.as_str().into() });
        ui.set_cpu(snap.cpu / 100.0);
        ui.set_cpu_text(if snap.warming_up { "…".into() } else { pct(snap.cpu) });
        ui.set_cores(ModelRc::new(VecModel::from(snap.cores.iter().map(|c| c / 100.0).collect::<Vec<f32>>())));
        ui.set_cpu_curve(ModelRc::new(VecModel::from(cpu_curve)));
        ui.set_gpu_curve(ModelRc::new(VecModel::from(gpu_curve)));
        ui.set_ram(snap.ram_percent() / 100.0);
        ui.set_ram_text(format!("{} / {}", fmt_mb(snap.ram_used_mb), fmt_mb(snap.ram_total_mb)).into());
        ui.set_swap_text(if snap.swap_total_mb > 0 { format!("Mémoire d'échange : {} / {}", fmt_mb(snap.swap_used_mb), fmt_mb(snap.swap_total_mb)) } else { String::new() }.into());
        let gpus: Vec<PerfGpuRow> = snap
            .gpus
            .iter()
            .map(|g| PerfGpuRow {
                name: g.name.as_str().into(),
                usage: g.usage.map(|u| u / 100.0).unwrap_or(-1.0),
                usage_text: g.usage.map(pct).unwrap_or_else(|| "non fourni par le pilote".into()),
                vram: match (g.vram_used_mb, g.vram_total_mb) {
                    (Some(u), Some(t)) if t > 0 => format!("Mémoire vidéo {} / {}", fmt_mb(u), fmt_mb(t)),
                    (Some(u), _) => format!("Mémoire vidéo {}", fmt_mb(u)),
                    _ => String::new(),
                }
                .into(),
            })
            .collect();
        ui.set_gpus(ModelRc::new(VecModel::from(gpus)));
        if !keep_disks {
            let disks: Vec<PerfDiskRow> = snap
                .disks
                .iter()
                .map(|d| {
                    let used = if d.total_mb > 0 { 1.0 - d.free_mb as f32 / d.total_mb as f32 } else { 0.0 };
                    PerfDiskRow { mount: d.mount.as_str().into(), free: format!("{} libres sur {}", fmt_mb(d.free_mb), fmt_mb(d.total_mb)).into(), used, warn: used > 0.9 }
                })
                .collect();
            ui.set_disks(ModelRc::new(VecModel::from(disks)));
        }
        let apps: Vec<PerfAppRow> = snap
            .apps
            .iter()
            .take(8)
            .map(|a| PerfAppRow {
                name: a.name.as_str().into(),
                count: if a.processes > 1 { format!("{} processus", a.processes) } else { String::new() }.into(),
                cpu: format!("{:.1} %", a.cpu).replace('.', ",").into(),
                ram: fmt_mb(a.ram_mb).into(),
            })
            .collect();
        ui.set_apps(ModelRc::new(VecModel::from(apps)));
        if let Some(own) = &snap.own {
            ui.set_own_text(format!("Halcyon lui-même : {} de mémoire, {:.1} % du processeur", fmt_mb(own.ram_mb), own.cpu).replace('.', ",").into());
        }
        // Recommandations : seulement quand la moyenne est significative.
        if stable {
            let ctx = RecoContext { performance_mode: self.performance_active(), game_running: self.active_session.get().is_some(), stable };
            let recos = {
                let st = self.perfc.borrow();
                sysmon::recommend(&snap, &st.history, ctx)
            };
            let rows: Vec<PerfRecoRow> = recos
                .iter()
                .map(|r| PerfRecoRow {
                    title: r.title.as_str().into(),
                    detail: r.detail.as_str().into(),
                    warning: r.level == Level::Warning,
                    action: match r.action {
                        Some(RecoAction::EnablePerformanceMode) => "perf-mode".into(),
                        None => SharedString::new(),
                    },
                })
                .collect();
            ui.set_recos(ModelRc::new(VecModel::from(rows)));
            ui.set_recos_ready(true);
        }
    }

    pub(crate) fn perf_reco_action(&self, action: &str) {
        if action == "perf-mode" && !self.performance_active() {
            self.set_performance(true);
        }
    }

    // ------------------------------------------------------------------ historique des parties

    /// Début d'une partie : échantillons espacés, résumé enregistré à la fin.
    pub(crate) fn perf_session_start(&self, game: GameId) {
        if !self.settings.borrow().perf_history {
            return;
        }
        let db = self.db.clone();
        let started_at = halcyon_storage::now_unix();
        let worker = spawn("perf-session", move |stop| {
            let mut m = Monitor::new();
            let mut acc = sysmon::SessionAccumulator::default();
            let begin = std::time::Instant::now();
            // Premier échantillon (référence du processeur), puis toutes les 10 s.
            let _ = m.sample(false, false);
            while !stop.load(Ordering::Relaxed) {
                std::thread::park_timeout(SESSION_INTERVAL);
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                acc.add(&m.sample(false, false));
            }
            if acc.samples < 3 {
                return;
            }
            let p = halcyon_storage::PerfSession {
                game_id: game,
                started_at,
                duration_secs: begin.elapsed().as_secs(),
                samples: acc.samples,
                cpu_avg: acc.cpu_avg(),
                cpu_max: acc.cpu_max,
                ram_avg_mb: acc.ram_avg_mb(),
                ram_max_mb: acc.ram_max_mb,
                ram_total_mb: acc.ram_total_mb,
                gpu_avg: acc.gpu_avg(),
                gpu_max: acc.gpu_max,
                vram_max_mb: acc.vram_max_mb,
            };
            if let Err(e) = db.lock().unwrap_or_else(|e| e.into_inner()).record_perf_session(&p) {
                log::error!("historique des performances : {e}");
            }
            post(|c| c.refresh_perf_sessions());
        });
        if let Some(old) = std::mem::replace(&mut self.perfc.borrow_mut().session, worker) {
            old.stop();
        }
    }

    pub(crate) fn perf_session_stop(&self) {
        if let Some(w) = self.perfc.borrow_mut().session.take() {
            w.stop();
        }
    }

    pub(crate) fn refresh_perf_sessions(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<PerfUi>();
        ui.set_history_enabled(self.settings.borrow().perf_history);
        let list = self.db().perf_sessions(None, 30).unwrap_or_else(|e| {
            log::error!("{e}");
            Vec::new()
        });
        let lib = self.lib.borrow();
        let rows: Vec<PerfSessionRow> = list
            .iter()
            .map(|p| PerfSessionRow {
                id: p.game_id as i32,
                game: lib.get(p.game_id).map(|g| g.name.clone()).unwrap_or_else(|| "Jeu retiré".into()).into(),
                date: halcyon_core::util::format_date(p.started_at).into(),
                duration: halcyon_core::util::format_playtime(p.duration_secs).into(),
                cpu: format!("{:.0} % (max {:.0} %)", p.cpu_avg, p.cpu_max).into(),
                ram: format!("{} (max {})", fmt_mb(p.ram_avg_mb), fmt_mb(p.ram_max_mb)).into(),
                gpu: match (p.gpu_avg, p.gpu_max) {
                    (Some(a), Some(m)) => format!("{a:.0} % (max {m:.0} %)"),
                    _ => "non mesurée".into(),
                }
                .into(),
                vram: p.vram_max_mb.map(fmt_mb).unwrap_or_default().into(),
            })
            .collect();
        ui.set_sessions(ModelRc::new(VecModel::from(rows)));
    }

    pub(crate) fn set_perf_history(&self, on: bool) {
        self.settings.borrow_mut().perf_history = on;
        self.save_settings();
        if !on {
            self.perf_session_stop();
        }
        self.refresh_perf_sessions();
    }

    // ------------------------------------------------------------------ temps d'image

    /// Analyse un journal choisi (ou le plus récent du dossier mémorisé).
    pub(crate) fn perf_open_frame_log(self: &Rc<Self>, latest: bool) {
        let path = if latest {
            let dir = self.settings.borrow().frametime_dir.clone();
            match dir.as_deref().and_then(frametimes::latest_log) {
                Some(p) => p,
                None => {
                    self.perf_frame_error("Aucun journal CSV dans le dossier choisi.");
                    return;
                }
            }
        } else {
            let Some(p) = rfd::FileDialog::new().set_title("Journal de temps d'image (MangoHud, PresentMon)").add_filter("CSV", &["csv"]).pick_file() else { return };
            if let Some(dir) = p.parent() {
                self.settings.borrow_mut().frametime_dir = Some(dir.to_path_buf());
                self.save_settings();
            }
            p
        };
        let file = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        std::thread::Builder::new()
            .name("frametimes".into())
            .spawn(move || {
                let r = frametimes::analyze_file(&path).map(|(log, s)| (log.format, log.application, frametimes::downsample_max(&log.frametimes_ms, 120), s));
                post(move |c| c.perf_frame_done(file, r));
            })
            .ok();
    }

    fn perf_frame_error(&self, msg: &str) {
        if let Some(app) = self.app() {
            let ui = app.global::<PerfUi>();
            ui.set_frame_ready(false);
            ui.set_frame_error(msg.into());
        }
    }

    fn perf_frame_done(&self, file: String, r: Result<(frametimes::LogFormat, String, Vec<f64>, frametimes::Summary), String>) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<PerfUi>();
        match r {
            Ok((format, application, bars, s)) => {
                let worst = bars.iter().cloned().fold(1.0, f64::max);
                let ratio: Vec<f32> = bars.iter().map(|b| (b / worst) as f32).collect();
                let title = if application.is_empty() { format!("{file} · {}", format.label()) } else { format!("{application} · {}", format.label()) };
                let num = |v: f64| format!("{v:.1}").replace('.', ",");
                ui.set_frame_title(title.into());
                ui.set_frame_avg(num(s.avg_fps).into());
                ui.set_frame_low1(num(s.low1_fps).into());
                ui.set_frame_low01(num(s.low01_fps).into());
                ui.set_frame_p99(format!("{} ms", num(s.p99_ms)).into());
                ui.set_frame_max(format!("{} ms", num(s.max_ms)).into());
                ui.set_frame_info(format!("{} images sur {} · {} saccade{} (image > 2 × la médiane de {} ms)", s.frames, halcyon_core::util::format_playtime(s.duration_secs as u64), s.stutters, if s.stutters > 1 { "s" } else { "" }, num(s.median_ms)).into());
                ui.set_frame_bars(ModelRc::new(VecModel::from(ratio)));
                ui.set_frame_worst(format!("{} ms", num(worst)).into());
                ui.set_frame_error(SharedString::new());
                ui.set_frame_ready(true);
            }
            Err(e) => self.perf_frame_error(&format!("{file} : {e}")),
        }
    }
}
