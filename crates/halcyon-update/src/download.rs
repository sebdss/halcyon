//! Téléchargement d'un paquet : en tâche de fond (l'appelant le lance dans son propre fil
//! d'exécution), reprenable (HTTP Range), annulable, progression réelle (octets reçus,
//! vitesse, temps restant), vérifié (taille et SHA-256 du manifeste signé) avant d'être
//! renommé en fichier définitif. Un fichier altéré est supprimé.

use crate::manifest::Package;
use crate::network::{net_error, Net};
use crate::security;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Progress {
    pub done: u64,
    pub total: u64,
    /// Octets par seconde (moyenne glissante), 0 tant qu'aucune mesure n'est possible.
    pub speed: u64,
    /// Secondes restantes estimées à partir de la vitesse mesurée (`None` sans mesure).
    pub eta: Option<u64>,
    /// Le téléchargement a repris un fichier partiel.
    pub resumed: bool,
}

impl Progress {
    pub fn ratio(&self) -> f32 {
        if self.total == 0 { 0.0 } else { (self.done as f64 / self.total as f64).min(1.0) as f32 }
    }
}

#[derive(Debug, PartialEq)]
pub enum DownloadError {
    Cancelled,
    Failed(String),
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadError::Cancelled => f.write_str("téléchargement annulé"),
            DownloadError::Failed(e) => f.write_str(e),
        }
    }
}

/// Description du fichier partiel : une reprise n'a lieu que pour le même paquet.
#[derive(Serialize, Deserialize, PartialEq)]
struct PartMeta {
    name: String,
    sha256: String,
    size: u64,
}

/// Mesure de débit sur une fenêtre glissante d'environ 3 secondes.
struct Meter {
    samples: Vec<(Instant, u64)>,
}

impl Meter {
    fn new(start: u64) -> Self {
        Meter { samples: vec![(Instant::now(), start)] }
    }

    fn push(&mut self, done: u64) -> u64 {
        let now = Instant::now();
        self.samples.push((now, done));
        while self.samples.len() > 2 && now.duration_since(self.samples[0].0) > Duration::from_secs(3) {
            self.samples.remove(0);
        }
        let (t0, d0) = self.samples[0];
        let dt = now.duration_since(t0).as_secs_f64();
        if dt < 0.5 { 0 } else { ((done - d0) as f64 / dt) as u64 }
    }
}

/// Options (tests : nombre d'essais et délai entre essais réduits).
#[derive(Clone, Copy)]
pub struct Retry {
    pub attempts: u32,
    pub delay: Duration,
}

impl Default for Retry {
    fn default() -> Self {
        Retry { attempts: 4, delay: Duration::from_secs(2) }
    }
}

/// Télécharge `pkg` depuis `url` dans `dir`. Renvoie le chemin du fichier vérifié.
pub fn download(net: &Net, url: &str, pkg: &Package, dir: &Path, cancel: &AtomicBool, retry: Retry, on: &mut dyn FnMut(Progress)) -> Result<PathBuf, DownloadError> {
    let fail = |e: String| DownloadError::Failed(e);
    std::fs::create_dir_all(dir).map_err(|e| fail(format!("{} : {e}", dir.display())))?;
    let dest = dir.join(&pkg.name);
    let part = dir.join(format!("{}.part", pkg.name));
    let meta_path = dir.join(format!("{}.part.json", pkg.name));
    let meta = PartMeta { name: pkg.name.clone(), sha256: pkg.sha256.clone(), size: pkg.size };

    // Déjà téléchargé et intact : rien à refaire.
    if dest.is_file() && security::sha256_file(&dest).ok().as_deref() == Some(pkg.sha256.as_str()) {
        on(Progress { done: pkg.size, total: pkg.size, ..Default::default() });
        return Ok(dest);
    }
    let _ = std::fs::remove_file(&dest);
    let same = std::fs::read(&meta_path).ok().and_then(|b| serde_json::from_slice::<PartMeta>(&b).ok()).is_some_and(|m| m == meta);
    if !same {
        let _ = std::fs::remove_file(&part);
    }
    std::fs::write(&meta_path, serde_json::to_vec(&meta).unwrap_or_default()).map_err(|e| fail(e.to_string()))?;

    let mut attempt = 0;
    loop {
        attempt += 1;
        match fetch_into(net, url, &part, pkg.size, cancel, on) {
            Ok(()) => break,
            Err(DownloadError::Cancelled) => return Err(DownloadError::Cancelled),
            Err(DownloadError::Failed(e)) if attempt >= retry.attempts => return Err(fail(format!("{e} (après {attempt} essais ; le téléchargement reprendra où il s'est arrêté)"))),
            Err(DownloadError::Failed(_)) => {
                // Attente interruptible avant de reprendre là où le transfert s'est arrêté.
                let until = Instant::now() + retry.delay * attempt;
                while Instant::now() < until {
                    if cancel.load(Ordering::SeqCst) {
                        return Err(DownloadError::Cancelled);
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
    }
    // Vérification avant tout usage : taille, puis empreinte du manifeste signé.
    let len = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    let actual = security::sha256_file(&part).map_err(|e| fail(e.to_string()))?;
    if len != pkg.size || actual != pkg.sha256 {
        let _ = std::fs::remove_file(&part);
        let _ = std::fs::remove_file(&meta_path);
        return Err(fail(format!("{} : empreinte SHA-256 différente du manifeste signé, fichier altéré supprimé", pkg.name)));
    }
    std::fs::rename(&part, &dest).map_err(|e| fail(e.to_string()))?;
    let _ = std::fs::remove_file(&meta_path);
    Ok(dest)
}

/// Un essai : reprend `part` s'il existe (Range), sinon repart de zéro.
fn fetch_into(net: &Net, url: &str, part: &Path, total: u64, cancel: &AtomicBool, on: &mut dyn FnMut(Progress)) -> Result<(), DownloadError> {
    let fail = |e: String| DownloadError::Failed(e);
    let mut have = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    if have > total {
        let _ = std::fs::remove_file(part);
        have = 0;
    }
    if have == total && total > 0 {
        return Ok(());
    }
    let mut req = net.agent.get(url);
    if have > 0 {
        req = req.header("Range", format!("bytes={have}-"));
    }
    let mut resp = req.call().map_err(|e| fail(net_error(&e)))?;
    let status = resp.status().as_u16();
    let resumed = match status {
        206 => {
            // Le serveur doit reprendre exactement où le fichier s'arrête.
            let ok = resp
                .headers()
                .get("content-range")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("bytes "))
                .and_then(|v| v.split('-').next())
                .and_then(|s| s.parse::<u64>().ok())
                == Some(have);
            if !ok {
                let _ = std::fs::remove_file(part);
                return Err(fail("reprise refusée par le serveur (plage incohérente) : nouvel essai depuis le début".into()));
            }
            true
        }
        200 => {
            // Serveur sans prise en charge de Range : on repart de zéro.
            have = 0;
            false
        }
        416 => {
            let _ = std::fs::remove_file(part);
            return Err(fail("reprise refusée par le serveur : nouvel essai depuis le début".into()));
        }
        s => return Err(fail(format!("téléchargement : réponse HTTP {s}"))),
    };
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(resumed)
        .truncate(!resumed)
        .open(part)
        .map_err(|e| fail(format!("{} : {e}", part.display())))?;
    let mut reader = resp.body_mut().with_config().limit(total - have + 1).reader();
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = have;
    let mut meter = Meter::new(done);
    let mut last = Instant::now() - Duration::from_secs(1);
    on(Progress { done, total, speed: 0, eta: None, resumed });
    loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = file.flush();
            return Err(DownloadError::Cancelled);
        }
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                let _ = file.flush();
                return Err(fail(format!("connexion interrompue : {e}")));
            }
        };
        if done + n as u64 > total {
            drop(file);
            let _ = std::fs::remove_file(part);
            return Err(fail("le serveur envoie plus de données que la taille signée : refusé".into()));
        }
        file.write_all(&buf[..n]).map_err(|e| fail(format!("écriture : {e}")))?;
        done += n as u64;
        if last.elapsed() >= Duration::from_millis(200) || done == total {
            last = Instant::now();
            let speed = meter.push(done);
            let eta = (speed > 0).then(|| (total - done).div_ceil(speed));
            on(Progress { done, total, speed, eta, resumed });
        }
    }
    file.flush().map_err(|e| fail(e.to_string()))?;
    file.sync_all().map_err(|e| fail(e.to_string()))?;
    if done < total {
        return Err(fail(format!("connexion interrompue à {done} octets sur {total}")));
    }
    Ok(())
}

/// « 4,2 Mo/s », « 1 min 20 s » : affichage.
pub fn format_speed(bps: u64) -> String {
    format!("{}/s", format_bytes(bps))
}

pub fn format_bytes(b: u64) -> String {
    let f = b as f64;
    let s = if f >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} Go", f / (1024.0 * 1024.0 * 1024.0))
    } else if f >= 1024.0 * 1024.0 {
        format!("{:.1} Mo", f / (1024.0 * 1024.0))
    } else if f >= 1024.0 {
        format!("{:.0} Ko", f / 1024.0)
    } else {
        format!("{b} o")
    };
    s.replace('.', ",")
}

pub fn format_eta(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs} s"),
        60..=3599 => format!("{} min {:02} s", secs / 60, secs % 60),
        _ => format!("{} h {:02} min", secs / 3600, (secs % 3600) / 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(format_bytes(1536 * 1024), "1,5 Mo");
        assert_eq!(format_speed(2048), "2 Ko/s");
        assert_eq!(format_eta(75), "1 min 15 s");
        assert_eq!(format_eta(3725), "1 h 02 min");
        let p = Progress { done: 5, total: 10, ..Default::default() };
        assert!((p.ratio() - 0.5).abs() < 1e-6);
    }
}
