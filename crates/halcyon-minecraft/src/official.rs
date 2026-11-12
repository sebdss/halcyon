//! Jouer par le launcher officiel de Minecraft (sans application Azure).
//!
//! Halcyon prépare tout (version, loader, mods, dossier de jeu), puis l'ajoute comme profil
//! dans le launcher officiel, comme le font les installateurs officiels de Fabric et de
//! Forge. La connexion et le lancement restent faits par le launcher officiel.
//!
//! Ce qui est lu : le nom du profil Minecraft du compte actif (affichage seulement). Les
//! jetons du launcher officiel ne sont jamais lus ni utilisés. Ce qui est écrit, avec
//! l'accord de l'utilisateur : les fichiers de version des loaders dans `versions/`, les
//! bibliothèques produites par Forge/NeoForge manquantes, et des profils `halcyon-…` dans
//! `launcher_profiles.json` (copie de sauvegarde faite avant la première modification).

use crate::install::{read_json, Layout};
use crate::meta::Env;
use crate::profiles::Profile;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Dossier du launcher officiel (`%APPDATA%\.minecraft`, `~/.minecraft`).
pub fn dir() -> Option<PathBuf> {
    let d = if cfg!(windows) { dirs::config_dir()?.join(".minecraft") } else { dirs::home_dir()?.join(".minecraft") };
    d.join("launcher_profiles.json").is_file().then_some(d)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LauncherKind {
    /// Application Microsoft Store / Xbox (Windows).
    Store,
    /// Installateur classique (`MinecraftLauncher.exe`, `minecraft-launcher`).
    Classic(&'static str),
}

/// Launcher officiel installé.
pub fn launcher() -> Option<(LauncherKind, PathBuf)> {
    if cfg!(windows) {
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(pf) = std::env::var_os(var) {
                let exe = PathBuf::from(pf).join("Minecraft Launcher").join("MinecraftLauncher.exe");
                if exe.is_file() {
                    return Some((LauncherKind::Classic("MinecraftLauncher.exe"), exe));
                }
            }
        }
        let store = dirs::data_local_dir()?.join("Packages").join("Microsoft.4297127D64EC6_8wekyb3d8bbwe");
        return store.is_dir().then_some((LauncherKind::Store, store));
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join("minecraft-launcher")).find(|p| p.is_file()).map(|p| (LauncherKind::Classic("minecraft-launcher"), p))
}

#[derive(Deserialize)]
struct Accounts {
    #[serde(default, rename = "activeAccountLocalId")]
    active: Option<String>,
    #[serde(default)]
    accounts: std::collections::BTreeMap<String, AccountEntry>,
}

/// Seuls ces champs sont lus (les jetons sont ignorés).
#[derive(Deserialize)]
struct AccountEntry {
    #[serde(default, rename = "minecraftProfile")]
    profile: Option<McProfile>,
}

#[derive(Deserialize)]
struct McProfile {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
}

/// Compte actif du launcher officiel : (pseudo, UUID).
pub fn active_account(dir: &Path) -> Option<(String, String)> {
    for f in ["launcher_accounts_microsoft_store.json", "launcher_accounts.json"] {
        let Ok(text) = std::fs::read_to_string(dir.join(f)) else { continue };
        let Ok(a) = serde_json::from_str::<Accounts>(&text) else { continue };
        let entry = a.active.as_ref().and_then(|id| a.accounts.get(id)).or_else(|| a.accounts.values().next());
        if let Some(p) = entry.and_then(|e| e.profile.as_ref()).filter(|p| !p.name.is_empty()) {
            return Some((p.name.clone(), p.id.clone()));
        }
    }
    None
}

pub fn profile_key(p: &Profile) -> String {
    format!("halcyon-{}", p.id)
}

fn iso_now() -> String {
    // Date ISO 8601 (UTC) attendue par le launcher.
    let secs = crate::profiles::now();
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Conversion jour julien → date civile (algorithme de Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.000Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Copie les fichiers nécessaires dans le dossier du launcher officiel et y ajoute (ou met à
/// jour) le profil de l'installation. `java_args` : mémoire et arguments JVM.
pub fn export_profile(layout: &Layout, official: &Path, p: &Profile, java_args: &str, resolution: Option<(u32, u32)>) -> std::io::Result<()> {
    // Fichiers de version des loaders (le launcher télécharge lui-même le vanilla).
    let mut id = p.version_id.clone();
    for _ in 0..5 {
        let src = layout.version_json(&id);
        let Ok(v) = read_json(&src) else { break };
        let dest = official.join("versions").join(&id).join(format!("{id}.json"));
        if v.get("inheritsFrom").is_some() || !dest.is_file() {
            std::fs::create_dir_all(dest.parent().unwrap_or(official))?;
            std::fs::copy(&src, &dest)?;
        }
        match v.get("inheritsFrom").and_then(|x| x.as_str()) {
            Some(parent) => id = parent.to_string(),
            None => break,
        }
    }
    // Bibliothèques produites localement par Forge/NeoForge (sans adresse de téléchargement).
    if let Ok(v) = crate::install::resolve_local(layout, &p.version_id) {
        for f in v.library_files(&Env::current()).into_iter().filter(|f| f.url.is_empty()) {
            let (src, dest) = (layout.libraries().join(&f.path), official.join("libraries").join(&f.path));
            if src.is_file() && !dest.is_file() {
                std::fs::create_dir_all(dest.parent().unwrap_or(official))?;
                std::fs::copy(&src, &dest)?;
            }
        }
    }
    let file = official.join("launcher_profiles.json");
    let backup = official.join("launcher_profiles.json.halcyon-backup");
    if !backup.is_file() {
        std::fs::copy(&file, &backup)?;
    }
    let text = std::fs::read_to_string(&file)?;
    let mut root: serde_json::Value = serde_json::from_str(&text).map_err(|e| std::io::Error::other(format!("launcher_profiles.json illisible : {e}")))?;
    if !root.get("profiles").is_some_and(|x| x.is_object()) {
        root["profiles"] = serde_json::json!({});
    }
    let key = profile_key(p);
    let created = root["profiles"].get(&key).and_then(|x| x.get("created")).cloned().unwrap_or_else(|| serde_json::Value::String(iso_now()));
    let mut entry = serde_json::json!({
        "name": format!("Halcyon · {}", p.name),
        "type": "custom",
        "created": created,
        "lastUsed": iso_now(),
        "lastVersionId": p.version_id,
        "gameDir": p.game_dir(layout).display().to_string(),
        "icon": "Furnace",
        "javaArgs": java_args,
    });
    if let Some((w, h)) = resolution {
        entry["resolution"] = serde_json::json!({"width": w, "height": h});
    }
    root["profiles"][&key] = entry;
    let tmp = file.with_extension("json.halcyon-part");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&root).unwrap_or_default())?;
    std::fs::rename(&tmp, &file)
}

/// Retire le profil Halcyon du launcher officiel (les autres profils ne sont pas touchés).
pub fn remove_profile(official: &Path, p: &Profile) -> std::io::Result<()> {
    let file = official.join("launcher_profiles.json");
    let mut root: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file)?).map_err(std::io::Error::other)?;
    if let Some(map) = root.get_mut("profiles").and_then(|x| x.as_object_mut()) {
        if map.remove(&profile_key(p)).is_none() {
            return Ok(());
        }
    }
    let tmp = file.with_extension("json.halcyon-part");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&root).unwrap_or_default())?;
    std::fs::rename(&tmp, &file)
}

/// Ouvre le launcher officiel.
pub fn open(kind: LauncherKind, path: &Path) -> std::io::Result<()> {
    match kind {
        LauncherKind::Store => {
            std::process::Command::new("explorer.exe").arg(r"shell:AppsFolder\Microsoft.4297127D64EC6_8wekyb3d8bbwe!Minecraft").spawn()?;
        }
        LauncherKind::Classic(_) => {
            std::process::Command::new(path).spawn()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loaders::Loader;
    use crate::profiles;

    #[test]
    fn reads_only_the_profile_name() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(
            t.path().join("launcher_accounts_microsoft_store.json"),
            r#"{"activeAccountLocalId":"a1","accounts":{"a1":{"accessToken":"NE-PAS-LIRE","minecraftProfile":{"id":"0123","name":"Sebi"},"type":"Xbox"}}}"#,
        )
        .unwrap();
        assert_eq!(active_account(t.path()), Some(("Sebi".to_string(), "0123".to_string())));
        assert!(active_account(&t.path().join("absent")).is_none());
    }

    #[test]
    fn exports_and_removes_only_our_profile() {
        let t = tempfile::tempdir().unwrap();
        let l = Layout::new(t.path().join("halcyon"));
        let off = t.path().join(".minecraft");
        std::fs::create_dir_all(&off).unwrap();
        std::fs::write(off.join("launcher_profiles.json"), r#"{"profiles":{"perso":{"name":"Mon profil","type":"custom","lastVersionId":"1.8"}},"settings":{"x":1},"version":3}"#).unwrap();
        std::fs::create_dir_all(l.version_dir("fabric-x")).unwrap();
        std::fs::write(l.version_json("fabric-x"), r#"{"id":"fabric-x","inheritsFrom":"1.20.1","mainClass":"K","libraries":[]}"#).unwrap();
        let p = profiles::new_profile(&l, "Fabric 1.20.1", "1.20.1", Loader::Fabric, "0.15", "fabric-x");
        export_profile(&l, &off, &p, "-Xmx4096M", Some((1280, 720))).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(off.join("launcher_profiles.json")).unwrap()).unwrap();
        assert_eq!(v["profiles"]["perso"]["name"], "Mon profil", "profil existant conservé");
        assert_eq!(v["settings"]["x"], 1);
        assert_eq!(v["profiles"]["halcyon-fabric-1.20.1"]["lastVersionId"], "fabric-x");
        assert!(v["profiles"]["halcyon-fabric-1.20.1"]["lastUsed"].as_str().unwrap().ends_with('Z'));
        assert!(off.join("versions/fabric-x/fabric-x.json").is_file());
        assert!(off.join("launcher_profiles.json.halcyon-backup").is_file());
        remove_profile(&off, &p).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(off.join("launcher_profiles.json")).unwrap()).unwrap();
        assert!(v["profiles"].get("halcyon-fabric-1.20.1").is_none());
        assert_eq!(v["profiles"]["perso"]["lastVersionId"], "1.8");
        assert_eq!(iso_now().len(), 24);
    }
}
