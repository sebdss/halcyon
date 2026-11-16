//! Journal des événements sensibles (`logs/security.log`).
//!
//! Une ligne JSON par événement : `{"t":…, "event":"…", "detail":"…"}`. Sont enregistrés
//! les changements de secrets (nom de la clé seulement), l'installation, l'activation et
//! la suppression d'extensions avec leurs autorisations, les mises à jour de Halcyon
//! (vérification d'empreinte et de signature), les connexions et déconnexions Microsoft,
//! et les modifications du fichier du launcher Minecraft officiel.
//!
//! **Jamais de valeur secrète** : les appels passent des identifiants, pas des contenus.
//! Le journal est limité à 512 Kio (un fichier précédent `security.log.1` est conservé).
//! Fichier lisible par l'utilisateur seul sous Unix.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const MAX_BYTES: u64 = 512 * 1024;

static SINK: OnceLock<Mutex<PathBuf>> = OnceLock::new();

/// À appeler une fois au démarrage ; sans appel, les événements ne sont pas écrits
/// (tests, outils en ligne de commande).
pub fn init(logs_dir: &Path) {
    let _ = std::fs::create_dir_all(logs_dir);
    let _ = SINK.set(Mutex::new(logs_dir.join("security.log")));
}

/// Enregistre un événement. N'échoue jamais (le journal ne doit pas bloquer une action).
pub fn event(event: &str, detail: &str) {
    let Some(sink) = SINK.get() else { return };
    let Ok(path) = sink.lock() else { return };
    let line = format_line(crate::now_unix(), event, detail);
    if std::fs::metadata(&*path).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
        let _ = std::fs::rename(&*path, path.with_extension("log.1"));
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    if let Ok(mut f) = opts.open(&*path) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Ligne JSON (sans retour chariot dans les champs : impossible de forger une entrée).
pub fn format_line(t: i64, event: &str, detail: &str) -> String {
    let v = serde_json::json!({ "t": t, "event": event, "detail": detail });
    format!("{v}\n")
}

/// Dernières entrées (les plus récentes en premier), pour l'écran Sécurité.
pub fn recent(logs_dir: &Path, max: usize) -> Vec<(i64, String, String)> {
    let mut out = Vec::new();
    for name in ["security.log", "security.log.1"] {
        let Ok(text) = std::fs::read_to_string(logs_dir.join(name)) else { continue };
        for l in text.lines().rev() {
            if out.len() >= max {
                return out;
            }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(l) {
                out.push((
                    v["t"].as_i64().unwrap_or(0),
                    v["event"].as_str().unwrap_or("").to_string(),
                    v["detail"].as_str().unwrap_or("").to_string(),
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_cannot_be_forged() {
        let l = format_line(1, "plugin.install", "x\n{\"t\":0,\"event\":\"fake\"}");
        assert_eq!(l.matches('\n').count(), 1, "le retour chariot du détail est échappé");
        let v: serde_json::Value = serde_json::from_str(l.trim()).unwrap();
        assert_eq!(v["event"], "plugin.install");
    }

    #[test]
    fn recent_reads_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let text = format!("{}{}", format_line(1, "a", ""), format_line(2, "b", "d"));
        std::fs::write(dir.path().join("security.log"), text).unwrap();
        let r = recent(dir.path(), 10);
        assert_eq!(r[0], (2, "b".into(), "d".into()));
        assert_eq!(r.len(), 2);
    }
}
