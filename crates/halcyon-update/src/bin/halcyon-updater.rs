//! Halcyon Updater : exécutable indépendant qui installe une mise à jour vérifiée pendant
//! que Halcyon est fermé, contrôle le démarrage de la nouvelle version et restaure
//! l'ancienne en cas d'échec. Lancé par Halcyon depuis une copie (`.hus/runner`), il n'est
//! jamais remplacé pendant qu'il travaille. Il ne télécharge rien et n'exécute aucun script.
//!
//! ```text
//! halcyon-updater apply   --root <dossier> --current <version> [--wait-pid <pid>] [--health-timeout <s>] [--no-relaunch]
//! halcyon-updater recover --root <dossier> --target <zip|appimage:<fichier>|migrate> [--wait-pid <pid>] [--no-relaunch]
//! halcyon-updater --version
//! ```
//!
//! Codes de sortie : 0 mise à jour installée (ou rien à faire), 2 version précédente
//! restaurée, 3 échec sans modification, 1 erreur d'utilisation.
#![cfg_attr(windows, windows_subsystem = "windows")]

use halcyon_update::install::Target;
use halcyon_update::recovery::{self, Layout};
use halcyon_update::runner::{self, ApplyOptions};
use halcyon_update::{security, State, Version};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

fn value(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn run(args: &[String]) -> Result<State, String> {
    let root = value(args, "--root").map(PathBuf::from).ok_or("--root manquant")?;
    if !root.is_dir() {
        return Err(format!("{} : dossier introuvable", root.display()));
    }
    let wait_pid = value(args, "--wait-pid").map(|p| p.parse::<u32>().map_err(|_| "--wait-pid invalide")).transpose()?;
    let relaunch = !args.iter().any(|a| a == "--no-relaunch");
    match args.first().map(String::as_str) {
        Some("apply") => {
            let current: Version = value(args, "--current").ok_or("--current manquant")?.parse()?;
            let health_timeout = Duration::from_secs(value(args, "--health-timeout").and_then(|s| s.parse().ok()).unwrap_or(120).clamp(5, 900));
            runner::apply(&ApplyOptions { root, wait_pid, current, health_timeout, keys: security::trusted_keys(), relaunch })
        }
        Some("recover") => {
            let target = Target::from_arg(&value(args, "--target").ok_or("--target manquant")?).ok_or("--target invalide")?;
            runner::recover(&root, wait_pid, &target, relaunch)
        }
        _ => Err("commande attendue : apply ou recover".into()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        println!("halcyon-updater {}", runner::updater_version());
        return ExitCode::SUCCESS;
    }
    match run(&args) {
        Ok(State::Completed) | Ok(State::Idle) | Ok(State::UpToDate) | Ok(State::Staging) => ExitCode::SUCCESS,
        Ok(State::RolledBack) => ExitCode::from(2),
        Ok(_) => ExitCode::from(3),
        Err(e) => {
            if let Some(root) = value(&args, "--root") {
                recovery::log(&Layout::new(root), &format!("updater : {e}"));
            }
            eprintln!("halcyon-updater : {e}");
            ExitCode::from(1)
        }
    }
}
