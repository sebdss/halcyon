//! Notifications système pour les succès débloqués.
//!
//! Une notification du système d'exploitation ne prend jamais le focus : elle
//! s'affiche par-dessus le jeu sans l'interrompre (contrairement à une fenêtre).

use std::path::Path;

/// Identifiant d'application Windows déclaré par l'installeur (raccourci du menu Démarrer).
#[cfg(windows)]
const APP_ID: &str = "Halcyon.Launcher";

/// Notification d'information (mises à jour…).
pub fn info(title: &str, body: &str) {
    let mut n = notify_rust::Notification::new();
    n.summary(title).body(body).appname("Halcyon").timeout(notify_rust::Timeout::Milliseconds(6000));
    #[cfg(windows)]
    if std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("unins000.exe").exists())).unwrap_or(false) {
        n.app_id(APP_ID);
    }
    std::thread::spawn(move || {
        if let Err(e) = n.show() {
            log::warn!("notification système impossible : {e}");
        }
    });
}

pub fn achievement(name: &str, description: &str, game: &str, rarity: Option<&str>, icon: Option<&Path>) {
    let mut body = String::new();
    if !description.is_empty() {
        body.push_str(description);
        body.push('\n');
    }
    body.push_str(game);
    if let Some(r) = rarity {
        body.push_str(" · ");
        body.push_str(r);
    }
    let mut n = notify_rust::Notification::new();
    n.summary(&format!("🏆 {name}")).body(&body).appname("Halcyon").timeout(notify_rust::Timeout::Milliseconds(6000));
    if let Some(icon) = icon {
        n.icon(&icon.display().to_string());
    }
    #[cfg(windows)]
    {
        // Version installée : l'installeur enregistre l'AppUserModelID de Halcyon.
        let installed = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|d| d.join("unins000.exe").exists()))
            .unwrap_or(false);
        if installed {
            n.app_id(APP_ID);
        }
    }
    // Hors du thread de l'interface : l'envoi peut prendre quelques millisecondes (D-Bus).
    std::thread::spawn(move || {
        if let Err(e) = n.show() {
            log::warn!("notification système impossible : {e}");
        }
    });
}
