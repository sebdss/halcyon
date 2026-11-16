//! Faux Halcyon des tests d'intégration (fonctionnalité `fault-injection`, jamais livré).
//!
//! Sa version est lue à la fin de son propre fichier (`HUSVER:<version>`, ajouté par les
//! tests). Variables : `HUS_FAKE_LOG` (journal des démarrages), `HUS_FAKE_CRASH_VERSION`
//! (version qui échoue au démarrage), `HUS_FAKE_SILENT_VERSION` (version qui démarre sans
//! jamais confirmer sa santé).

use halcyon_update::recovery::{self, Layout, ARG_HEALTH, ENV_ROOT};
use std::io::Write;

fn own_version() -> String {
    let exe = std::env::current_exe().ok().and_then(|p| std::fs::read(p).ok()).unwrap_or_default();
    let tail = &exe[exe.len().saturating_sub(64)..];
    let text = String::from_utf8_lossy(tail);
    text.rsplit("HUSVER:").next().map(|s| s.trim_end_matches(['\0', ' ']).trim().to_string()).unwrap_or_default()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let version = own_version();
    if let Ok(log) = std::env::var("HUS_FAKE_LOG") {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log) {
            let _ = writeln!(f, "start {version} {}", args.join(" "));
        }
    }
    if std::env::var("HUS_FAKE_CRASH_VERSION").ok().as_deref() == Some(version.as_str()) {
        std::process::exit(101);
    }
    if std::env::var("HUS_FAKE_SILENT_VERSION").ok().as_deref() == Some(version.as_str()) {
        std::thread::sleep(std::time::Duration::from_secs(30));
        return;
    }
    if let (Some(i), Ok(root)) = (args.iter().position(|a| a == ARG_HEALTH), std::env::var(ENV_ROOT)) {
        if let Some(tx) = args.get(i + 1) {
            let _ = halcyon_update::runner::on_launcher_start(&Layout::new(root), Some(tx));
            let _ = recovery::now();
        }
    }
}
