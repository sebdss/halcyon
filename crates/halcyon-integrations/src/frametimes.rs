//! FPS et temps d'image, **quand ils sont techniquement disponibles**.
//!
//! Mesurer les images d'un jeu demande soit d'injecter du code dans son rendu (ce que
//! Halcyon refuse : anti-triche, stabilité), soit un outil spécialisé du système. Halcyon
//! lit donc les journaux produits par ces outils :
//! - **MangoHud** (Linux) : journal CSV (`output_folder` + `toggle_logging`, ou
//!   `autostart_log`) — colonne `frametime` en millisecondes ;
//! - **PresentMon** (Windows, Intel, basé sur ETW, sans injection) : CSV avec
//!   `MsBetweenPresents` (format 1.x) ou `FrameTime` (2.x).
//!
//! Aucun chiffre n'est extrapolé : sans journal, la page indique que la mesure n'est pas
//! disponible.

use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogFormat {
    MangoHud,
    PresentMon,
}

impl LogFormat {
    pub fn label(self) -> &'static str {
        match self {
            LogFormat::MangoHud => "MangoHud",
            LogFormat::PresentMon => "PresentMon",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FrameLog {
    pub format: LogFormat,
    /// Application indiquée par le journal (PresentMon), sinon vide.
    pub application: String,
    /// Temps d'image en millisecondes, dans l'ordre.
    pub frametimes_ms: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Summary {
    pub frames: usize,
    pub duration_secs: f64,
    pub avg_fps: f64,
    /// « 1 % low » : 1000 / 99ᵉ centile du temps d'image.
    pub low1_fps: f64,
    /// « 0,1 % low » : 1000 / 99,9ᵉ centile.
    pub low01_fps: f64,
    pub median_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
    /// Images plus de deux fois plus longues que la médiane (saccades perceptibles).
    pub stutters: usize,
}

/// Lit un journal ; le format est reconnu à son en-tête.
pub fn parse(text: &str) -> Result<FrameLog, String> {
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let cols: Vec<String> = split_csv(line).into_iter().map(|c| c.trim().to_string()).collect();
        let find = |name: &str| cols.iter().position(|c| c.eq_ignore_ascii_case(name));
        // MangoHud d'abord : sa colonne « frametime » ressemble au « FrameTime » de PresentMon 2.
        let (format, col) = if let (true, Some(c)) = (cols.first().is_some_and(|c| c == "fps"), find("frametime")) {
            (LogFormat::MangoHud, c)
        } else if let Some(c) = find("MsBetweenPresents").or_else(|| find("FrameTime")) {
            (LogFormat::PresentMon, c)
        } else {
            continue;
        };
        let app_col = find("Application");
        let mut application = String::new();
        let mut frametimes_ms = Vec::new();
        for row in &lines[i + 1..] {
            let fields = split_csv(row);
            let Some(v) = fields.get(col).and_then(|v| v.trim().parse::<f64>().ok()) else { continue };
            // Valeurs aberrantes (pause, chargement de plus de 10 s) : ignorées.
            if v.is_finite() && v > 0.0 && v < 10_000.0 {
                frametimes_ms.push(v);
                if application.is_empty() {
                    if let Some(a) = app_col.and_then(|c| fields.get(c)) {
                        application = a.trim().to_string();
                    }
                }
            }
        }
        if frametimes_ms.is_empty() {
            return Err(tr!("journal {label} sans images mesurées", label = format.label()));
        }
        return Ok(FrameLog { format, application, frametimes_ms });
    }
    Err(tr!("format non reconnu : journal MangoHud ou PresentMon (CSV) attendu"))
}

/// Champs d'une ligne CSV (guillemets doubles gérés, pas de retour chariot dans un champ).
fn split_csv(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

pub fn summarize(ft: &[f64]) -> Option<Summary> {
    if ft.is_empty() {
        return None;
    }
    let mut sorted = ft.to_vec();
    sorted.sort_by(f64::total_cmp);
    let pct = |p: f64| sorted[(((sorted.len() - 1) as f64) * p).round() as usize];
    let total: f64 = ft.iter().sum();
    let median = pct(0.5);
    Some(Summary {
        frames: ft.len(),
        duration_secs: total / 1000.0,
        avg_fps: ft.len() as f64 * 1000.0 / total,
        low1_fps: 1000.0 / pct(0.99),
        low01_fps: 1000.0 / pct(0.999),
        median_ms: median,
        p99_ms: pct(0.99),
        max_ms: *sorted.last().unwrap_or(&0.0),
        stutters: ft.iter().filter(|&&f| f > median * 2.0).count(),
    })
}

/// Réduit la courbe à `n` barres en gardant le **pire** temps de chaque tranche (les
/// saccades restent visibles).
pub fn downsample_max(ft: &[f64], n: usize) -> Vec<f64> {
    if ft.is_empty() || n == 0 {
        return Vec::new();
    }
    if ft.len() <= n {
        return ft.to_vec();
    }
    (0..n)
        .map(|i| {
            let a = i * ft.len() / n;
            let b = ((i + 1) * ft.len() / n).max(a + 1);
            ft[a..b].iter().cloned().fold(0.0, f64::max)
        })
        .collect()
}

/// Journal le plus récent d'un dossier (fichiers `.csv`), pour l'analyse après une partie.
pub fn latest_log(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("csv")))
        // Les résumés MangoHud (`*_summary.csv`) ne contiennent pas les images.
        .filter(|e| !e.file_name().to_string_lossy().to_lowercase().ends_with("_summary.csv"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .max_by_key(|(t, _)| *t)
        .map(|(_, p)| p)
}

/// Lit et résume un fichier (taille bornée : 64 Mo).
pub fn analyze_file(path: &Path) -> Result<(FrameLog, Summary), String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > 64 * 1024 * 1024 {
        return Err(tr!("journal trop volumineux (plus de 64 Mo)"));
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let log = parse(&text)?;
    let s = summarize(&log.frametimes_ms).ok_or(tr!("journal vide"))?;
    Ok((log, s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mangohud_log() {
        let text = "os,cpu,gpu,ram,kernel,driver,cpuscheduler\nArch Linux,AMD Ryzen 7 5800X,AMD Radeon RX 6800,32 GB,6.11,Mesa 24.2,\n--------------------FRAME METRICS--------------------\nfps,frametime,cpu_load,gpu_load,cpu_temp,gpu_temp,gpu_core_clock,gpu_mem_clock,gpu_vram_used,gpu_power,ram_used,elapsed\n60.1,16.64,30,90,60,70,2300,1000,4.1,200,8.2,16640000\n59.8,16.72,31,91,60,70,2300,1000,4.1,200,8.2,33360000\n30.0,33.3,31,91,60,70,2300,1000,4.1,200,8.2,66660000\n";
        let log = parse(text).unwrap();
        assert_eq!(log.format, LogFormat::MangoHud);
        assert_eq!(log.frametimes_ms, vec![16.64, 16.72, 33.3]);
    }

    #[test]
    fn parses_presentmon_v1_and_v2() {
        let v1 = "Application,ProcessID,SwapChainAddress,Runtime,SyncInterval,PresentFlags,Dropped,TimeInSeconds,MsBetweenPresents,MsBetweenDisplayChange\n\"Game.exe\",1234,0x1,DXGI,0,0,0,0.0,8.33,8.33\nGame.exe,1234,0x1,DXGI,0,0,0,0.01,8.40,8.40\n";
        let log = parse(v1).unwrap();
        assert_eq!(log.format, LogFormat::PresentMon);
        assert_eq!(log.application, "Game.exe");
        assert_eq!(log.frametimes_ms, vec![8.33, 8.40]);
        let v2 = "Application,ProcessID,SwapChainAddress,PresentRuntime,SyncInterval,PresentFlags,AllowsTearing,PresentMode,FrameType,CPUStartTime,FrameTime,CPUBusy\nGame.exe,1,0x1,DXGI,0,0,1,Hardware: Independent Flip,Application,0.1,6.94,5.0\n";
        assert_eq!(parse(v2).unwrap().frametimes_ms, vec![6.94]);
        assert!(parse("a,b,c\n1,2,3\n").is_err());
        assert!(parse("fps,frametime\n").is_err(), "en-tête seul");
    }

    #[test]
    fn summary_statistics() {
        // 980 images à 10 ms, 20 à 40 ms.
        let mut ft = vec![10.0; 980];
        ft.extend(vec![40.0; 20]);
        let s = summarize(&ft).unwrap();
        assert_eq!(s.frames, 1000);
        assert!((s.duration_secs - 10.6).abs() < 1e-9);
        assert!((s.avg_fps - 1000.0 * 1000.0 / 10_600.0).abs() < 1e-9);
        assert_eq!(s.median_ms, 10.0);
        assert_eq!(s.p99_ms, 40.0);
        assert_eq!(s.low1_fps, 25.0);
        assert_eq!(s.max_ms, 40.0);
        assert_eq!(s.stutters, 20);
        assert!(summarize(&[]).is_none());
    }

    #[test]
    fn downsampling_keeps_spikes() {
        let mut ft = vec![10.0; 1000];
        ft[503] = 90.0;
        let d = downsample_max(&ft, 100);
        assert_eq!(d.len(), 100);
        assert_eq!(d.iter().cloned().fold(0.0, f64::max), 90.0);
        assert_eq!(downsample_max(&[1.0, 2.0], 100), vec![1.0, 2.0]);
    }

    #[test]
    fn latest_log_skips_summaries() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("game_2026-11-16_summary.csv"), "x").unwrap();
        std::fs::write(dir.path().join("game_2026-11-16.csv"), "x").unwrap();
        std::fs::write(dir.path().join("notes.txt"), "x").unwrap();
        assert_eq!(latest_log(dir.path()).unwrap().file_name().unwrap(), "game_2026-11-16.csv");
    }
}
