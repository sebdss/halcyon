//! Intégration au bureau : démarrage avec la session, raccourcis de jeux.
//!
//! Windows : clé `HKCU\…\Run` (aucun droit administrateur) et raccourcis `.lnk`
//! créés par le composant système WScript.Shell. Linux : fichiers `.desktop`
//! (spécification freedesktop), dans `~/.config/autostart` et `~/.local/share/applications`.

use std::path::{Path, PathBuf};

const RUN_VALUE: &str = "Halcyon";

fn exe() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| tr!("chemin de Halcyon introuvable : {e}", e = e))
}

#[allow(unused_imports)]
pub use halcyon_core::desktop::{desktop_quote, safe_file_name};

#[cfg(not(windows))]
fn autostart_file() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("autostart/halcyon.desktop"))
}

pub fn autostart_enabled() -> bool {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        winreg::RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
            .and_then(|k| k.get_value::<String, _>(RUN_VALUE))
            .is_ok()
    }
    #[cfg(not(windows))]
    {
        let _ = RUN_VALUE;
        autostart_file().is_some_and(|f| f.is_file())
    }
}

/// Active ou désactive le lancement à l'ouverture de session (`--minimized`).
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
        let (key, _) = winreg::RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey_with_flags("Software\\Microsoft\\Windows\\CurrentVersion\\Run", KEY_SET_VALUE)
            .map_err(|e| e.to_string())?;
        if enabled {
            let cmd = format!("\"{}\" --minimized", exe()?.display());
            key.set_value(RUN_VALUE, &cmd).map_err(|e| e.to_string())
        } else {
            match key.delete_value(RUN_VALUE) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.to_string()),
            }
        }
    }
    #[cfg(not(windows))]
    {
        let file = autostart_file().ok_or(tr!("dossier de configuration introuvable"))?;
        if !enabled {
            return match std::fs::remove_file(&file) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.to_string()),
            };
        }
        let content = halcyon_core::desktop::autostart_entry(&exe()?.display().to_string());
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&file, content).map_err(|e| e.to_string())
    }
}

/// Crée un raccourci qui lance le jeu via Halcyon (`--launch <id>`) sur le bureau.
/// Renvoie le chemin créé.
pub fn create_game_shortcut(name: &str, id: i64, icon: Option<&Path>) -> Result<PathBuf, String> {
    let exe = exe()?;
    let desktop = dirs::desktop_dir().ok_or(tr!("dossier Bureau introuvable"))?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = icon;
        let dest = desktop.join(format!("{}.lnk", safe_file_name(name)));
        // Valeurs transmises par variables d'environnement : aucune interprétation par PowerShell.
        let script = "$s=(New-Object -ComObject WScript.Shell).CreateShortcut($env:HALCYON_LNK);\
                      $s.TargetPath=$env:HALCYON_EXE;$s.Arguments=$env:HALCYON_ARGS;\
                      $s.IconLocation=$env:HALCYON_EXE;$s.Description=$env:HALCYON_DESC;$s.Save()";
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script])
            .env("HALCYON_LNK", &dest)
            .env("HALCYON_EXE", &exe)
            .env("HALCYON_ARGS", format!("--launch {id}"))
            .env("HALCYON_DESC", tr!("Lancer {name} avec Halcyon", name = name))
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_err(|e| tr!("PowerShell indisponible : {e}", e = e))?;
        if status.success() && dest.is_file() { Ok(dest) } else { Err(tr!("création du raccourci refusée par le système")) }
    }
    #[cfg(not(windows))]
    {
        let icon = icon.filter(|p| p.is_file()).map(|p| p.display().to_string()).unwrap_or_else(|| "halcyon".into());
        let clean = |s: &str| s.replace(['\n', '\r'], " ");
        let content = tr!("[Desktop Entry]\nType=Application\nName={name}\nComment=Lancer avec Halcyon\nExec={display} --launch {id}\nIcon={icon}\nTerminal=false\nCategories=Game;\n", name = clean(name), display = desktop_quote(&exe.display().to_string()), id = id, icon = clean(&icon));
        let file_name = tr!("halcyon-jeu-{id}.desktop", id = id);
        // Menu des applications, puis Bureau (fichier exécutable : requis par les environnements de bureau).
        if let Some(apps) = dirs::data_dir().map(|d| d.join("applications")) {
            let _ = std::fs::create_dir_all(&apps);
            let _ = std::fs::write(apps.join(&file_name), &content);
        }
        std::fs::create_dir_all(&desktop).map_err(|e| e.to_string())?;
        let dest = desktop.join(&file_name);
        std::fs::write(&dest, &content).map_err(|e| e.to_string())?;
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755));
        Ok(dest)
    }
}
