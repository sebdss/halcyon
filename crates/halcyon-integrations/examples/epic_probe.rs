//! Diagnostic : jeux Epic détectés sur cette machine.
//! `cargo run -p halcyon-integrations --example epic_probe [dossier des manifestes]`
use halcyon_core::scanner::{GameScanner, ScanContext};
use halcyon_integrations::epic::EpicScanner;

fn main() {
    let dir = std::env::args().nth(1).map(std::path::PathBuf::from);
    let scanner = EpicScanner::new(dir);
    println!("dossier : {:?}", scanner.manifests_dir());
    match scanner.scan(&ScanContext::silent()) {
        Ok(r) => {
            for g in &r.games {
                println!("jeu : {} ({}) installé={}", g.name, g.source_key, g.installed);
            }
            for w in &r.warnings {
                println!("avertissement : {w}");
            }
        }
        Err(e) => println!("erreur : {e}"),
    }
}
