//! Surveillance du système pour le Performance Center : processeur, mémoire, cartes
//! graphiques, disques et applications les plus gourmandes.
//!
//! Tout passe par `sysinfo` (déjà utilisé pour détecter les jeux en cours) :
//! - processeur et mémoire : API du système (Windows) et `/proc` (Linux) ;
//! - cartes graphiques (fonctionnalité `gpu` de sysinfo) : compteurs de performance
//!   « GPU Engine » et D3DKMT sous Windows ; sous Linux `sysfs` (AMD), NVML chargée à la
//!   demande (NVIDIA) et Vulkan pour le reste. Une valeur que le pilote n'expose pas reste
//!   **absente** (jamais estimée) ;
//! - aucune mesure ne tourne si personne ne la demande : la page en cours d'affichage
//!   échantillonne toutes les 1,5 s, une partie toutes les 10 s, rien sinon.
//!
//! Les recommandations sont des constats tirés des mesures, avec une seule action
//! automatique, réversible : activer le mode Performance de Halcyon. Halcyon ne ferme
//! jamais un programme et ne modifie aucun réglage du système.

use std::collections::HashMap;
use sysinfo::{CpuRefreshKind, ProcessRefreshKind, ProcessesToUpdate, System};

const MIB: u64 = 1024 * 1024;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GpuStat {
    pub name: String,
    /// Utilisation (0–100), si le pilote la fournit.
    pub usage: Option<f32>,
    pub vram_used_mb: Option<u64>,
    pub vram_total_mb: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DiskStat {
    pub mount: String,
    pub total_mb: u64,
    pub free_mb: u64,
    pub removable: bool,
}

/// Une application : processus de même nom regroupés (un navigateur en lance des dizaines).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AppUsage {
    pub name: String,
    pub processes: u32,
    /// Part du processeur entier (0–100, toutes les unités de calcul confondues).
    pub cpu: f32,
    pub ram_mb: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub cpu_name: String,
    /// Utilisation globale (0–100). 0 au tout premier échantillon (mesure différentielle).
    pub cpu: f32,
    pub cores: Vec<f32>,
    pub ram_total_mb: u64,
    pub ram_used_mb: u64,
    pub swap_total_mb: u64,
    pub swap_used_mb: u64,
    pub gpus: Vec<GpuStat>,
    pub disks: Vec<DiskStat>,
    /// Applications triées par poids décroissant (vide si non demandé).
    pub apps: Vec<AppUsage>,
    /// Halcyon lui-même (transparence sur son propre coût).
    pub own: Option<AppUsage>,
    /// Le premier échantillon du processeur n'est pas encore significatif.
    pub warming_up: bool,
}

impl Snapshot {
    pub fn ram_percent(&self) -> f32 {
        if self.ram_total_mb == 0 {
            0.0
        } else {
            self.ram_used_mb as f32 * 100.0 / self.ram_total_mb as f32
        }
    }

    /// Utilisation de la carte graphique la plus chargée.
    pub fn gpu_max(&self) -> Option<f32> {
        self.gpus.iter().filter_map(|g| g.usage).fold(None, |a, u| Some(a.map_or(u, |a: f32| a.max(u))))
    }

    pub fn vram_max_mb(&self) -> Option<u64> {
        self.gpus.iter().filter_map(|g| g.vram_used_mb).max()
    }
}

/// Source de mesures. À garder entre deux échantillons (mesures différentielles).
pub struct Monitor {
    sys: Option<System>,
    gpus: Option<sysinfo::Gpus>,
    cpu_name: String,
    samples: u64,
    own_pid: u32,
}

impl Default for Monitor {
    fn default() -> Self {
        Self::new()
    }
}

impl Monitor {
    pub fn new() -> Self {
        let mut sys = System::new().map_err(|e| log::warn!("surveillance système indisponible : {e}")).ok();
        let mut cpu_name = String::new();
        if let Some(s) = sys.as_mut() {
            s.refresh_cpu_list(CpuRefreshKind::nothing().with_usage());
            cpu_name = s.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default();
        }
        let gpus = sysinfo::Gpus::new_with_refreshed_list().map_err(|e| log::info!("cartes graphiques non mesurables : {e}")).ok();
        Self { sys, gpus, cpu_name, samples: 0, own_pid: std::process::id() }
    }

    /// Nouvel échantillon. `apps` : liste des applications (plus coûteux : tous les processus).
    pub fn sample(&mut self, apps: bool, disks: bool) -> Snapshot {
        self.samples += 1;
        let mut snap = Snapshot { cpu_name: self.cpu_name.clone(), warming_up: self.samples < 2, ..Default::default() };
        if let Some(sys) = self.sys.as_mut() {
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            snap.cpu = sys.global_cpu_usage().clamp(0.0, 100.0);
            snap.cores = sys.cpus().iter().map(|c| c.usage().clamp(0.0, 100.0)).collect();
            snap.ram_total_mb = sys.total_memory() / MIB;
            snap.ram_used_mb = sys.used_memory() / MIB;
            snap.swap_total_mb = sys.total_swap() / MIB;
            snap.swap_used_mb = sys.used_swap() / MIB;
            if apps {
                sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_cpu().with_memory());
                let ncpu = sys.cpus().len().max(1) as f32;
                let mut raw = Vec::new();
                for (pid, p) in sys.processes() {
                    // Linux : les fils d'exécution apparaissent comme des processus.
                    if p.thread_kind().is_some() {
                        continue;
                    }
                    let name = p.name().to_string_lossy().to_string();
                    let entry = (name, p.cpu_usage() / ncpu, p.memory() / MIB);
                    if pid.as_u32() == self.own_pid {
                        snap.own = Some(AppUsage { name: "Halcyon".into(), processes: 1, cpu: entry.1, ram_mb: entry.2 });
                    } else {
                        raw.push(entry);
                    }
                }
                snap.apps = group_processes(raw, snap.ram_total_mb);
            }
        }
        if let Some(g) = self.gpus.as_mut() {
            g.refresh(true);
            snap.gpus = g
                .iter()
                .map(|g| GpuStat {
                    name: g.model().or(g.vendor()).unwrap_or("Carte graphique").to_string(),
                    usage: g.usage().map(|u| u.clamp(0.0, 100.0)),
                    vram_used_mb: g.used_memory().map(|v| v / MIB),
                    vram_total_mb: g.total_memory().map(|v| v / MIB),
                })
                .collect();
        }
        if disks {
            if let Ok(list) = sysinfo::Disks::new_with_refreshed_list_specifics(sysinfo::DiskRefreshKind::nothing().with_storage()) {
                let mut seen = std::collections::HashSet::new();
                snap.disks = list
                    .iter()
                    .filter(|d| d.total_space() > 1024 * MIB)
                    // Même volume monté plusieurs fois (Flatpak, conteneurs) : une seule ligne.
                    .filter(|d| seen.insert(d.name().to_string_lossy().to_string() + &d.total_space().to_string()))
                    .map(|d| DiskStat { mount: d.mount_point().display().to_string(), total_mb: d.total_space() / MIB, free_mb: d.available_space() / MIB, removable: d.is_removable() })
                    .collect();
            }
        }
        snap
    }
}

/// Regroupe les processus par nom et trie par poids (processeur, puis mémoire relative).
pub fn group_processes(raw: Vec<(String, f32, u64)>, ram_total_mb: u64) -> Vec<AppUsage> {
    let mut map: HashMap<String, AppUsage> = HashMap::new();
    for (name, cpu, ram) in raw {
        let key = display_name(&name);
        if key.is_empty() {
            continue;
        }
        let e = map.entry(key.to_lowercase()).or_insert_with(|| AppUsage { name: key.clone(), ..Default::default() });
        e.processes += 1;
        e.cpu += cpu.max(0.0);
        e.ram_mb += ram;
    }
    let total = ram_total_mb.max(1) as f32;
    let weight = |a: &AppUsage| a.cpu + a.ram_mb as f32 * 100.0 / total;
    let mut v: Vec<AppUsage> = map.into_values().collect();
    v.sort_by(|a, b| weight(b).total_cmp(&weight(a)).then_with(|| a.name.cmp(&b.name)));
    v.truncate(12);
    v
}

/// « chrome.exe » → « chrome » ; les noms tronqués de Linux restent tels quels.
fn display_name(name: &str) -> String {
    let n = name.trim();
    let lower = n.to_lowercase();
    if lower.ends_with(".exe") {
        n[..n.len() - 4].to_string()
    } else {
        n.to_string()
    }
}

/// Moyennes glissantes des derniers échantillons (constats stables, pas de pics isolés).
#[derive(Clone, Debug, Default)]
pub struct History {
    cpu: Vec<f32>,
    gpu: Vec<f32>,
    cap: usize,
}

impl History {
    pub fn new(cap: usize) -> Self {
        Self { cap: cap.max(1), ..Default::default() }
    }

    pub fn push(&mut self, s: &Snapshot) {
        if s.warming_up {
            return;
        }
        push_capped(&mut self.cpu, s.cpu, self.cap);
        if let Some(g) = s.gpu_max() {
            push_capped(&mut self.gpu, g, self.cap);
        }
    }

    pub fn cpu(&self) -> &[f32] {
        &self.cpu
    }

    pub fn gpu(&self) -> &[f32] {
        &self.gpu
    }

    pub fn cpu_avg(&self) -> Option<f32> {
        avg(&self.cpu)
    }

    pub fn gpu_avg(&self) -> Option<f32> {
        avg(&self.gpu)
    }
}

fn push_capped(v: &mut Vec<f32>, x: f32, cap: usize) {
    if v.len() == cap {
        v.remove(0);
    }
    v.push(x);
}

fn avg(v: &[f32]) -> Option<f32> {
    (!v.is_empty()).then(|| v.iter().sum::<f32>() / v.len() as f32)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Warning,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoAction {
    /// Activer le mode Performance de Halcyon (désactivable à tout moment).
    EnablePerformanceMode,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Recommendation {
    pub id: &'static str,
    pub level: Level,
    pub title: String,
    pub detail: String,
    pub action: Option<RecoAction>,
}

/// Contexte de Halcyon utile aux recommandations.
#[derive(Clone, Copy, Debug, Default)]
pub struct RecoContext {
    pub performance_mode: bool,
    pub game_running: bool,
    /// Assez d'échantillons pour une moyenne fiable.
    pub stable: bool,
}

/// Constats tirés des mesures. Seuils volontairement prudents : une recommandation doit
/// rester vraie quelques secondes plus tard.
pub fn recommend(s: &Snapshot, h: &History, ctx: RecoContext) -> Vec<Recommendation> {
    let mut out = Vec::new();
    let ram = s.ram_percent();
    if s.ram_total_mb > 0 && ram >= 85.0 {
        let top: Vec<String> = top_by_ram(&s.apps, 3).iter().map(|a| format!("{} ({})", a.name, fmt_mb(a.ram_mb))).collect();
        out.push(Recommendation {
            id: "ram-high",
            level: Level::Warning,
            title: format!("Mémoire presque pleine ({ram:.0} %)"),
            detail: if top.is_empty() { "Fermez les applications inutiles avant de jouer.".into() } else { format!("Plus gros consommateurs : {}. Fermez celles dont vous n'avez pas besoin avant de jouer.", top.join(", ")) },
            action: None,
        });
    }
    if s.swap_used_mb >= 1024 && ram >= 75.0 {
        out.push(Recommendation {
            id: "swap",
            level: Level::Warning,
            title: format!("{} de mémoire d'échange utilisés", fmt_mb(s.swap_used_mb)),
            detail: "Le système déplace de la mémoire vers le disque : cela provoque des saccades en jeu.".into(),
            action: None,
        });
    }
    if ctx.stable && !ctx.game_running {
        for a in s.apps.iter().filter(|a| a.cpu >= 20.0).take(2) {
            out.push(Recommendation {
                id: "cpu-app",
                level: Level::Warning,
                title: format!("{} utilise {:.0} % du processeur", a.name, a.cpu),
                detail: "Hors partie, cette charge réduira la marge disponible pour vos jeux.".into(),
                action: None,
            });
        }
        if let Some(g) = h.gpu_avg().filter(|g| *g >= 40.0) {
            out.push(Recommendation {
                id: "gpu-busy",
                level: Level::Info,
                title: format!("Carte graphique occupée ({g:.0} % en moyenne) sans partie en cours"),
                detail: "Une application (vidéo, navigateur, enregistrement) utilise la carte graphique.".into(),
                action: None,
            });
        }
    }
    for d in s.disks.iter().filter(|d| !d.removable && d.total_mb > 0) {
        let pct = d.free_mb as f32 * 100.0 / d.total_mb as f32;
        if pct < 10.0 || d.free_mb < 10 * 1024 {
            out.push(Recommendation {
                id: "disk-low",
                level: if d.free_mb < 5 * 1024 { Level::Warning } else { Level::Info },
                title: format!("Peu d'espace libre sur {} ({})", d.mount, fmt_mb(d.free_mb)),
                detail: "Les mises à jour de jeux et la mémoire virtuelle ont besoin d'espace libre.".into(),
                action: None,
            });
        }
    }
    if !ctx.performance_mode {
        out.push(Recommendation {
            id: "perf-mode",
            level: Level::Info,
            title: "Mode Performance de Halcyon désactivé".into(),
            detail: "Il coupe animations et effets de Halcyon et reporte ses tâches secondaires. Désactivable à tout moment.".into(),
            action: Some(RecoAction::EnablePerformanceMode),
        });
    }
    out
}

fn top_by_ram(apps: &[AppUsage], n: usize) -> Vec<&AppUsage> {
    let mut v: Vec<&AppUsage> = apps.iter().collect();
    v.sort_by_key(|a| std::cmp::Reverse(a.ram_mb));
    v.truncate(n);
    v
}

/// « 512 Mo », « 3,4 Go ».
pub fn fmt_mb(mb: u64) -> String {
    if mb >= 1024 {
        format!("{:.1} Go", mb as f64 / 1024.0).replace('.', ",")
    } else {
        format!("{mb} Mo")
    }
}

/// Cumul des mesures d'une partie (un échantillon toutes les 10 s).
#[derive(Clone, Debug, Default)]
pub struct SessionAccumulator {
    pub samples: u32,
    cpu_sum: f64,
    pub cpu_max: f32,
    ram_sum: u64,
    pub ram_max_mb: u64,
    pub ram_total_mb: u64,
    gpu_sum: f64,
    gpu_n: u32,
    pub gpu_max: Option<f32>,
    pub vram_max_mb: Option<u64>,
}

impl SessionAccumulator {
    pub fn add(&mut self, s: &Snapshot) {
        if s.warming_up {
            return;
        }
        self.samples += 1;
        self.cpu_sum += s.cpu as f64;
        self.cpu_max = self.cpu_max.max(s.cpu);
        self.ram_sum += s.ram_used_mb;
        self.ram_max_mb = self.ram_max_mb.max(s.ram_used_mb);
        self.ram_total_mb = s.ram_total_mb;
        if let Some(g) = s.gpu_max() {
            self.gpu_sum += g as f64;
            self.gpu_n += 1;
            self.gpu_max = Some(self.gpu_max.map_or(g, |m| m.max(g)));
        }
        if let Some(v) = s.vram_max_mb() {
            self.vram_max_mb = Some(self.vram_max_mb.map_or(v, |m| m.max(v)));
        }
    }

    pub fn cpu_avg(&self) -> f32 {
        if self.samples == 0 {
            0.0
        } else {
            (self.cpu_sum / self.samples as f64) as f32
        }
    }

    pub fn ram_avg_mb(&self) -> u64 {
        self.ram_sum.checked_div(self.samples as u64).unwrap_or(0)
    }

    pub fn gpu_avg(&self) -> Option<f32> {
        (self.gpu_n > 0).then(|| (self.gpu_sum / self.gpu_n as f64) as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(cpu: f32, ram_used: u64, gpu: Option<f32>) -> Snapshot {
        Snapshot { cpu, ram_total_mb: 16384, ram_used_mb: ram_used, gpus: vec![GpuStat { name: "G".into(), usage: gpu, ..Default::default() }], ..Default::default() }
    }

    #[test]
    fn groups_processes_by_name() {
        let raw = vec![("chrome.exe".into(), 5.0, 300), ("Chrome.exe".into(), 7.0, 500), ("steam".into(), 1.0, 200), ("".into(), 50.0, 1)];
        let g = group_processes(raw, 16384);
        assert_eq!(g[0].name, "chrome");
        assert_eq!(g[0].processes, 2);
        assert_eq!(g[0].cpu, 12.0);
        assert_eq!(g[0].ram_mb, 800);
        assert_eq!(g.len(), 2, "processus sans nom ignorés");
    }

    #[test]
    fn recommendations_follow_measurements() {
        let mut s = snap(30.0, 15000, Some(80.0));
        s.apps = vec![AppUsage { name: "obs".into(), processes: 1, cpu: 35.0, ram_mb: 4000 }];
        s.disks = vec![DiskStat { mount: "C:\\".into(), total_mb: 500_000, free_mb: 3000, removable: false }, DiskStat { mount: "E:\\".into(), total_mb: 64_000, free_mb: 100, removable: true }];
        let mut h = History::new(10);
        for _ in 0..5 {
            h.push(&s);
        }
        let r = recommend(&s, &h, RecoContext { performance_mode: false, game_running: false, stable: true });
        let ids: Vec<&str> = r.iter().map(|r| r.id).collect();
        assert_eq!(ids, ["ram-high", "cpu-app", "gpu-busy", "disk-low", "perf-mode"]);
        assert!(r[0].detail.contains("obs (3,9 Go)"));
        assert_eq!(r[3].level, Level::Warning);
        assert_eq!(r[4].action, Some(RecoAction::EnablePerformanceMode));

        // En partie : la charge du jeu n'est pas un problème à signaler.
        let r = recommend(&s, &h, RecoContext { performance_mode: true, game_running: true, stable: true });
        assert!(r.iter().all(|r| r.id != "cpu-app" && r.id != "gpu-busy" && r.id != "perf-mode"));

        // Système sain : aucune alerte.
        let ok = snap(5.0, 4000, Some(2.0));
        assert!(recommend(&ok, &History::new(5), RecoContext { performance_mode: true, stable: true, ..Default::default() }).is_empty());
    }

    #[test]
    fn history_and_session_ignore_warmup_and_missing_gpu() {
        let mut h = History::new(3);
        let mut acc = SessionAccumulator::default();
        let mut w = snap(0.0, 1000, None);
        w.warming_up = true;
        h.push(&w);
        acc.add(&w);
        for (c, g) in [(10.0, Some(50.0)), (20.0, None), (30.0, Some(70.0)), (40.0, Some(90.0))] {
            let s = snap(c, 2000, g);
            h.push(&s);
            acc.add(&s);
        }
        assert_eq!(h.cpu(), &[20.0, 30.0, 40.0], "fenêtre glissante de 3");
        assert_eq!(acc.samples, 4);
        assert_eq!(acc.cpu_avg(), 25.0);
        assert_eq!(acc.cpu_max, 40.0);
        assert_eq!(acc.gpu_avg(), Some(70.0), "moyenne sur les seuls échantillons mesurés");
        assert_eq!(acc.gpu_max, Some(90.0));
        assert_eq!(SessionAccumulator::default().gpu_avg(), None);
    }

    #[test]
    fn real_monitor_samples_this_machine() {
        let mut m = Monitor::new();
        let _ = m.sample(true, true);
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        let s = m.sample(true, true);
        assert!(!s.warming_up);
        assert!(s.ram_total_mb > 0 && s.ram_used_mb <= s.ram_total_mb);
        assert!(!s.cores.is_empty());
        assert!(s.own.is_some(), "le processus de test se retrouve lui-même");
    }
}
