//! Jeu à distance via Sunshine (serveur) et Moonlight (client), solutions existantes
//! et éprouvées : Halcyon ne réimplémente aucun protocole de streaming. Il détecte
//! les logiciels, vérifie la disponibilité des machines et pilote le client Moonlight
//! en ligne de commande.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Port HTTP utilisé par Sunshine/GameStream pour l'appairage et la liste des jeux.
pub const GAMESTREAM_PORT: u16 = 47989;
/// Interface web de configuration de Sunshine.
pub const SUNSHINE_WEB: &str = "https://localhost:47990";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tool {
    /// Commande à exécuter (programme + arguments de préfixe, ex. `flatpak run …`).
    pub command: Vec<String>,
    pub via: String,
}

fn in_path(names: &[&str]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for n in names {
            for ext in if cfg!(windows) { &["", ".exe"][..] } else { &[""][..] } {
                let p = dir.join(format!("{n}{ext}"));
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    None
}

fn flatpak(app: &str) -> Option<Tool> {
    let exports = [
        PathBuf::from("/var/lib/flatpak/exports/bin").join(app),
        dirs::data_dir().map(|d| d.join("flatpak/exports/bin").join(app)).unwrap_or_default(),
    ];
    exports.iter().any(|p| p.exists()).then(|| Tool {
        command: vec!["flatpak".into(), "run".into(), app.into()],
        via: "Flatpak".into(),
    })
}

fn program_files(rel: &str) -> Option<PathBuf> {
    ["ProgramFiles", "ProgramFiles(x86)"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|p| PathBuf::from(p).join(rel))
        .find(|p| p.is_file())
}

pub fn detect_moonlight(forced: Option<&Path>) -> Option<Tool> {
    if let Some(p) = forced.filter(|p| p.is_file()) {
        return Some(Tool { command: vec![p.to_string_lossy().into_owned()], via: tr!("chemin indiqué").into() });
    }
    if let Some(p) = program_files("Moonlight Game Streaming/Moonlight.exe") {
        return Some(Tool { command: vec![p.to_string_lossy().into_owned()], via: "installation".into() });
    }
    if let Some(p) = in_path(&["moonlight", "moonlight-qt"]) {
        return Some(Tool { command: vec![p.to_string_lossy().into_owned()], via: "PATH".into() });
    }
    flatpak("com.moonlight_stream.Moonlight")
}

pub fn detect_sunshine() -> Option<Tool> {
    if let Some(p) = program_files("Sunshine/sunshine.exe") {
        return Some(Tool { command: vec![p.to_string_lossy().into_owned()], via: "installation".into() });
    }
    if let Some(p) = in_path(&["sunshine"]) {
        return Some(Tool { command: vec![p.to_string_lossy().into_owned()], via: "PATH".into() });
    }
    flatpak("dev.lizardbyte.app.Sunshine")
}

/// Fichier des applications publiées par Sunshine.
pub fn sunshine_apps_file() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(c) = dirs::config_dir() {
        candidates.push(c.join("sunshine/apps.json"));
    }
    if let Some(h) = dirs::home_dir() {
        candidates.push(h.join(".var/app/dev.lizardbyte.app.Sunshine/config/sunshine/apps.json"));
    }
    if let Some(pf) = std::env::var_os("ProgramFiles") {
        candidates.push(PathBuf::from(pf).join("Sunshine/config/apps.json"));
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// La machine répond-elle sur le port GameStream ?
pub fn host_reachable(address: &str) -> bool {
    let target = if address.contains(':') && !address.starts_with('[') { address.to_string() } else { format!("{address}:{GAMESTREAM_PORT}") };
    target
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.next())
        .is_some_and(|addr| TcpStream::connect_timeout(&addr, Duration::from_secs(2)).is_ok())
}

fn run(tool: &Tool, args: &[&str]) -> Result<String, String> {
    let out = Command::new(&tool.command[0])
        .args(&tool.command[1..])
        .args(args)
        .output()
        .map_err(|e| tr!("Moonlight n'a pas pu être lancé : {e}", e = e))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if out.status.success() {
        Ok(text)
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() { tr!("Moonlight a échoué (code {v})", v = out.status.code().unwrap_or(-1)) } else { err })
    }
}

/// Jeux publiés par une machine (`moonlight list <hôte>`).
pub fn list_apps(tool: &Tool, host: &str) -> Result<Vec<String>, String> {
    Ok(parse_list(&run(tool, &["list", host])?))
}

pub fn parse_list(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        // Lignes de diagnostic de Moonlight (préfixées « [ » ou horodatées) ignorées.
        .filter(|l| !l.starts_with('[') && !(l.len() > 8 && l.as_bytes()[2] == b':' && l.as_bytes()[5] == b':'))
        .map(str::to_string)
        .collect()
}

/// Démarre l'appairage : Moonlight affiche un code PIN à saisir dans Sunshine.
pub fn pair(tool: &Tool, host: &str) -> Result<(), String> {
    spawn(tool, &["pair", host])
}

pub fn stream(tool: &Tool, host: &str, app: &str) -> Result<(), String> {
    spawn(tool, &["stream", host, app])
}

pub fn quit(tool: &Tool, host: &str) -> Result<(), String> {
    run(tool, &["quit", host]).map(|_| ())
}

fn spawn(tool: &Tool, args: &[&str]) -> Result<(), String> {
    Command::new(&tool.command[0])
        .args(&tool.command[1..])
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|e| tr!("Moonlight n'a pas pu être lancé : {e}", e = e))
}

/// Machine configurée.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Host {
    pub name: String,
    pub address: String,
}

/// Ajoute (ou met à jour) un jeu dans `apps.json` de Sunshine. Une copie de sauvegarde
/// du fichier est créée avant toute modification.
pub fn publish_to_sunshine(apps_file: &Path, name: &str, command: &str, image: Option<&Path>) -> Result<PathBuf, String> {
    let text = std::fs::read_to_string(apps_file).map_err(|e| tr!("lecture de {apps_file} : {e}", apps_file = apps_file.display(), e = e))?;
    let mut v: Value = serde_json::from_str(&text).map_err(|e| tr!("{file} invalide : {e}", file = "apps.json", e = e))?;
    let backup = apps_file.with_extension("json.halcyon-backup");
    std::fs::write(&backup, &text).map_err(|e| tr!("copie de sauvegarde impossible : {e}", e = e))?;
    let apps = v.get_mut("apps").and_then(|a| a.as_array_mut()).ok_or_else(|| tr!("apps.json : liste « apps » absente"))?;
    apps.retain(|a| a.get("name").and_then(|n| n.as_str()) != Some(name));
    let mut entry = json!({ "name": name, "detached": [command], "auto-detach": "true" });
    if let Some(img) = image {
        entry["image-path"] = Value::String(img.to_string_lossy().into_owned());
    }
    apps.push(entry);
    let out = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    let tmp = apps_file.with_extension("json.tmp");
    std::fs::write(&tmp, out)
        .and_then(|_| std::fs::rename(&tmp, apps_file))
        .map_err(|e| tr!("écriture de apps.json impossible (droits administrateur nécessaires ?) : {e}", e = e))?;
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishes_with_backup_and_replaces_same_name() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("apps.json");
        std::fs::write(&f, r#"{"env": {}, "apps": [{"name": "Desktop"}, {"name": "Hades", "cmd": "old"}]}"#).unwrap();
        let backup = publish_to_sunshine(&f, "Hades", "steam://rungameid/1145360", None).unwrap();
        assert!(backup.is_file());
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        let apps = v["apps"].as_array().unwrap();
        assert_eq!(apps.len(), 2, "pas de doublon");
        assert_eq!(apps[1]["detached"][0], "steam://rungameid/1145360");
        assert_eq!(apps[0]["name"], "Desktop", "applications existantes conservées");
        assert!(publish_to_sunshine(&tmp.path().join("absent.json"), "x", "y", None).is_err());
    }

    #[test]
    fn parses_app_lists() {
        assert_eq!(parse_list("Desktop\nSteam Big Picture\n\nHades\n"), ["Desktop", "Steam Big Picture", "Hades"]);
        assert!(!host_reachable("127.0.0.1:1"));
    }
}
