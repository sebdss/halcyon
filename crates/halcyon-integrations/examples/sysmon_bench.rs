//! Coût des mesures du Performance Center sur cette machine.
//!
//! ```text
//! cargo run --release -p halcyon-integrations --example sysmon_bench [secondes]
//! ```
//!
//! Reproduit les deux cadences de Halcyon : page affichée (échantillon complet avec la liste
//! des applications, toutes les 1,5 s) et partie en cours (sans processus, toutes les 10 s,
//! accélérée ici à 1,5 s pour obtenir assez de mesures). Affiche la durée médiane d'un
//! échantillon et le temps processeur consommé par seconde. Sortie JSON sur stdout.

use halcyon_integrations::sysmon::Monitor;
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
fn cpu_time() -> Duration {
    // utime + stime du processus (en tics d'horloge, 100 par seconde sous Linux).
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let f: Vec<&str> = stat.rsplit_once(')').map(|(_, r)| r.split_whitespace().collect()).unwrap_or_default();
    let t = f.get(11).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0) + f.get(12).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    Duration::from_millis(t * 10)
}

#[cfg(not(target_os = "linux"))]
fn cpu_time() -> Duration {
    Duration::ZERO
}

fn run(label: &str, secs: u64, apps: bool) -> String {
    let mut m = Monitor::new();
    let _ = m.sample(apps, false);
    let mut times = Vec::new();
    let c0 = cpu_time();
    let t0 = Instant::now();
    let mut last = None;
    while t0.elapsed() < Duration::from_secs(secs) {
        std::thread::sleep(Duration::from_millis(1500));
        let t = Instant::now();
        last = Some(m.sample(apps, false));
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let cpu = cpu_time().saturating_sub(c0).as_secs_f64() / t0.elapsed().as_secs_f64() * 100.0;
    times.sort_by(f64::total_cmp);
    let med = times.get(times.len() / 2).copied().unwrap_or(0.0);
    let max = times.last().copied().unwrap_or(0.0);
    let s = last.unwrap_or_default();
    format!(
        "{{\"mode\":\"{label}\",\"samples\":{},\"sample_median_ms\":{med:.2},\"sample_max_ms\":{max:.2},\"cpu_pct_one_core\":{cpu:.3},\"gpus\":{},\"apps\":{},\"cores\":{}}}",
        times.len(),
        s.gpus.len(),
        s.apps.len(),
        s.cores.len()
    )
}

fn main() {
    let secs = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    println!("[{},", run("page (avec applications)", secs, true));
    println!(" {}]", run("partie (sans processus)", secs, false));
}
