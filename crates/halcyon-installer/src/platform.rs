//! Emplacements et intégration au système (menu, Bureau, démarrage, entrée de désinstallation).
//!
//! Tous les chemins passent par `Env` : les tests remplacent le dossier personnel par un
//! dossier temporaire et vérifient exactement ce qui est créé et retiré.

use std::path::{Path, PathBuf};

/// Emplacements utilisés par l'installation.
#[derive(Clone, Debug)]
pub struct Env {
    /// Dossier d'installation proposé par défaut (par utilisateur).
    pub default_dir: PathBuf,
    /// Linux : `~/.local/share` ; Windows : `%APPDATA%`.
    pub data_home: PathBuf,
    /// Linux : `~/.config` ; Windows : `%APPDATA%`.
    pub config_home: PathBuf,
    /// Linux : `~/.local/state` ; Windows : `%LOCALAPPDATA%`.
    pub state_home: PathBuf,
    /// Linux : `~/.cache` ; Windows : `%LOCALAPPDATA%`.
    pub cache_home: PathBuf,
    /// Linux : `~/.local/bin` (lien vers l'exécutable).
    pub bin_dir: PathBuf,
    pub desktop: Option<PathBuf>,
    /// Données de Halcyon (bibliothèque, paramètres, sauvegardes de jeux conservées).
    pub user_data: Vec<PathBuf>,
    /// Cache de Halcyon (miniatures, images téléchargées).
    pub user_cache: Vec<PathBuf>,
}

impl Env {
    pub fn system() -> Env {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let data_home = dirs::data_dir().unwrap_or_else(|| home.join(".local/share"));
        let local = dirs::data_local_dir().unwrap_or_else(|| data_home.clone());
        let cache_home = dirs::cache_dir().unwrap_or_else(|| home.join(".cache"));
        #[cfg(windows)]
        {
            Env {
                default_dir: local.join("Programs").join("Halcyon"),
                config_home: data_home.clone(),
                state_home: local.clone(),
                cache_home: cache_home.clone(),
                bin_dir: local.join("Programs").join("Halcyon"),
                desktop: dirs::desktop_dir(),
                user_data: vec![data_home.join("Halcyon")],
                user_cache: vec![local.join("Halcyon")],
                data_home,
            }
        }
        #[cfg(not(windows))]
        {
            Env {
                default_dir: home.join(".local/opt/halcyon"),
                config_home: dirs::config_dir().unwrap_or_else(|| home.join(".config")),
                state_home: dirs::state_dir().unwrap_or_else(|| home.join(".local/state")),
                bin_dir: dirs::executable_dir().unwrap_or_else(|| home.join(".local/bin")),
                // Sans user-dirs.dirs, `~/Desktop` s'il existe.
                desktop: dirs::desktop_dir().or_else(|| Some(home.join("Desktop")).filter(|p| p.is_dir())),
                user_data: vec![local.join("Halcyon")],
                user_cache: vec![cache_home.join("Halcyon")],
                cache_home,
                data_home,
            }
        }
    }

    /// Environnement isolé sous `root` (tests).
    pub fn rooted(root: &Path) -> Env {
        Env {
            default_dir: root.join("opt/halcyon"),
            data_home: root.join("share"),
            config_home: root.join("config"),
            state_home: root.join("state"),
            cache_home: root.join("cache"),
            bin_dir: root.join("bin"),
            desktop: Some(root.join("Desktop")),
            user_data: vec![root.join("share/Halcyon")],
            user_cache: vec![root.join("cache/Halcyon")],
        }
    }

    /// Fichier qui mémorise l'emplacement d'une installation personnalisée.
    pub fn pointer_file(&self) -> PathBuf {
        self.state_home.join("halcyon").join("installation.json")
    }
}

/// Nom de l'exécutable principal sur cette plateforme.
pub fn main_exe_name() -> &'static str {
    if cfg!(windows) { "Halcyon.exe" } else { "halcyon" }
}

/// Nom de l'installateur conservé dans le dossier (réparation, désinstallation).
pub fn maintenance_exe_name() -> &'static str {
    if cfg!(windows) { "halcyon-setup.exe" } else { "halcyon-setup" }
}

/// Plateforme de ce binaire, comparée à celle de la charge utile.
pub fn current_platform() -> String {
    let os = if cfg!(windows) { "windows" } else if cfg!(target_os = "linux") { "linux" } else { std::env::consts::OS };
    format!("{os}-{}", std::env::consts::ARCH)
}

// ---------------------------------------------------------------------------------- Linux

#[cfg(not(windows))]
pub mod linux {
    use super::Env;
    use halcyon_core::desktop::{app_entry, autostart_entry, desktop_quote};
    use std::path::{Path, PathBuf};

    pub fn menu_entry(env: &Env) -> PathBuf {
        env.data_home.join("applications/halcyon.desktop")
    }
    pub fn maintenance_entry(env: &Env) -> PathBuf {
        env.data_home.join("applications/halcyon-setup.desktop")
    }
    pub fn icon(env: &Env) -> PathBuf {
        env.data_home.join("icons/hicolor/256x256/apps/halcyon.png")
    }
    pub fn mime(env: &Env) -> PathBuf {
        env.data_home.join("mime/packages/halcyon.xml")
    }
    pub fn bin_link(env: &Env) -> PathBuf {
        env.bin_dir.join("halcyon")
    }
    pub fn desktop_shortcut(env: &Env) -> Option<PathBuf> {
        env.desktop.as_ref().map(|d| d.join("halcyon.desktop"))
    }
    pub fn autostart(env: &Env) -> PathBuf {
        env.config_home.join("autostart/halcyon.desktop")
    }

    fn write(path: &Path, content: &[u8], mode: u32) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(|e| format!("{} : {e}", d.display()))?;
        }
        let tmp = path.with_extension("halcyon-tmp");
        std::fs::write(&tmp, content).map_err(|e| format!("{} : {e}", path.display()))?;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode)).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| format!("{} : {e}", path.display()))
    }

    /// Crée les éléments demandés ; renvoie les chemins créés (retirés à la désinstallation).
    pub fn integrate(env: &Env, dir: &Path, menu: bool, desktop: bool, autostart_on: bool) -> Result<Vec<PathBuf>, String> {
        let exe = dir.join(super::main_exe_name());
        let exe_s = exe.display().to_string();
        let mut created = Vec::new();
        // Lien dans ~/.local/bin (remplace un exécutable copié par l'ancien install.sh).
        let link = bin_link(env);
        if let Some(d) = link.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        if link.symlink_metadata().is_ok() {
            std::fs::remove_file(&link).map_err(|e| format!("{} : {e}", link.display()))?;
        }
        std::os::unix::fs::symlink(&exe, &link).map_err(|e| format!("{} : {e}", link.display()))?;
        created.push(link);
        let icon_src = dir.join("halcyon.png");
        let icon_name = if icon_src.is_file() {
            let ic = icon(env);
            let bytes = std::fs::read(&icon_src).map_err(|e| e.to_string())?;
            write(&ic, &bytes, 0o644)?;
            created.push(ic);
            "halcyon".to_string()
        } else {
            exe_s.clone()
        };
        let mime_src = dir.join("halcyon-mime.xml");
        if mime_src.is_file() {
            let m = mime(env);
            let bytes = std::fs::read(&mime_src).map_err(|e| e.to_string())?;
            write(&m, &bytes, 0o644)?;
            created.push(m);
        }
        let entry = app_entry(&exe_s, &icon_name);
        if menu {
            let p = menu_entry(env);
            write(&p, entry.as_bytes(), 0o644)?;
            created.push(p);
            let setup = dir.join(super::maintenance_exe_name());
            let m = maintenance_entry(env);
            let content = format!(
                "[Desktop Entry]\nType=Application\nName=Gérer l'installation de Halcyon\n\
                 Comment=Réparer, mettre à jour ou désinstaller Halcyon\nExec={}\nIcon={icon_name}\nTerminal=false\nCategories=Settings;\n",
                desktop_quote(&setup.display().to_string())
            );
            write(&m, content.as_bytes(), 0o644)?;
            created.push(m);
        }
        if desktop {
            if let Some(p) = desktop_shortcut(env) {
                write(&p, entry.as_bytes(), 0o755)?;
                // GNOME : le raccourci est marqué « fiable » (sinon une confirmation s'affiche).
                let _ = std::process::Command::new("gio")
                    .args(["set", &p.display().to_string(), "metadata::trusted", "true"])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                created.push(p);
            }
        }
        if autostart_on {
            let p = autostart(env);
            write(&p, autostart_entry(&exe_s).as_bytes(), 0o644)?;
            created.push(p);
        }
        refresh_caches(env);
        Ok(created)
    }

    /// Bases du menu et des types de fichiers (si les outils sont présents).
    pub fn refresh_caches(env: &Env) {
        for (tool, dir) in [("update-desktop-database", env.data_home.join("applications")), ("update-mime-database", env.data_home.join("mime"))] {
            if dir.is_dir() {
                let _ = std::process::Command::new(tool)
                    .arg(&dir)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
        }
    }

    /// Retire un élément créé par l'installation, s'il désigne bien ce dossier.
    pub fn remove_if_ours(path: &Path, dir: &Path) -> bool {
        let Ok(meta) = path.symlink_metadata() else { return false };
        let ours = if meta.file_type().is_symlink() {
            std::fs::read_link(path).is_ok_and(|t| t.starts_with(dir))
        } else if path.extension().is_some_and(|e| e == "desktop") {
            std::fs::read_to_string(path).is_ok_and(|t| t.contains(&dir.display().to_string()))
        } else {
            true // icône, types de fichiers : propres à Halcyon
        };
        ours && std::fs::remove_file(path).is_ok()
    }
}

// -------------------------------------------------------------------------------- Windows

#[cfg(windows)]
pub mod windows {
    use super::Env;
    use std::path::{Path, PathBuf};
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_ALL_ACCESS};
    use winreg::RegKey;

    pub const UNINSTALL_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Halcyon";
    /// Clé de l'installateur Inno Setup des versions 0.1 à 0.4.0.
    pub const INNO_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{6B0E3B4F-2D7C-4E1A-9C5B-8F1A7D2E4C90}_is1";
    const AUMID: &str = "Halcyon.Launcher";

    fn root(all_users: bool) -> RegKey {
        RegKey::predef(if all_users { HKEY_LOCAL_MACHINE } else { HKEY_CURRENT_USER })
    }

    pub fn start_menu(all_users: bool) -> Option<PathBuf> {
        if all_users {
            std::env::var_os("ProgramData").map(|p| PathBuf::from(p).join("Microsoft\\Windows\\Start Menu\\Programs"))
        } else {
            dirs::data_dir().map(|d| d.join("Microsoft\\Windows\\Start Menu\\Programs"))
        }
    }

    /// Raccourci `.lnk` par le composant système WScript.Shell (valeurs passées par
    /// variables d'environnement : aucune interprétation par PowerShell).
    pub fn shortcut(lnk: &Path, exe: &Path, args: &str) -> Result<(), String> {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let script = "$s=(New-Object -ComObject WScript.Shell).CreateShortcut($env:H_LNK);\
                      $s.TargetPath=$env:H_EXE;$s.Arguments=$env:H_ARGS;$s.IconLocation=$env:H_EXE;\
                      $s.WorkingDirectory=$env:H_DIR;$s.Description='Halcyon';$s.Save()";
        if let Some(d) = lnk.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script])
            .env("H_LNK", lnk)
            .env("H_EXE", exe)
            .env("H_ARGS", args)
            .env("H_DIR", exe.parent().unwrap_or(exe))
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_err(|e| format!("PowerShell indisponible : {e}"))?;
        if status.success() && lnk.is_file() { Ok(()) } else { Err(format!("raccourci {} non créé", lnk.display())) }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn integrate(
        env: &Env,
        dir: &Path,
        version: &str,
        size_kb: u64,
        menu: bool,
        desktop: bool,
        autostart: bool,
        all_users: bool,
    ) -> Result<Vec<PathBuf>, String> {
        let exe = dir.join(super::main_exe_name());
        let setup = dir.join(super::maintenance_exe_name());
        let mut created = Vec::new();
        if menu {
            if let Some(sm) = start_menu(all_users) {
                let lnk = sm.join("Halcyon.lnk");
                shortcut(&lnk, &exe, "")?;
                created.push(lnk);
            }
        }
        if desktop {
            if let Some(d) = &env.desktop {
                let lnk = d.join("Halcyon.lnk");
                shortcut(&lnk, &exe, "")?;
                created.push(lnk);
            }
        }
        let r = root(all_users);
        let (key, _) = r.create_subkey(UNINSTALL_KEY).map_err(|e| format!("registre : {e}"))?;
        let q = |p: &Path| format!("\"{}\"", p.display());
        let set = |k: &RegKey, n: &str, v: &str| k.set_value(n, &v.to_string()).map_err(|e| format!("registre : {e}"));
        set(&key, "DisplayName", "Halcyon")?;
        set(&key, "DisplayVersion", version)?;
        set(&key, "Publisher", "Sebi")?;
        set(&key, "InstallLocation", &dir.display().to_string())?;
        set(&key, "DisplayIcon", &exe.display().to_string())?;
        set(&key, "UninstallString", &format!("{} --uninstall", q(&setup)))?;
        set(&key, "ModifyPath", &format!("{} --repair", q(&setup)))?;
        key.set_value("EstimatedSize", &(size_kb as u32)).map_err(|e| e.to_string())?;
        key.set_value("NoRepair", &0u32).map_err(|e| e.to_string())?;
        // Associations des thèmes et extensions (confirmation demandée par Halcyon à l'ouverture).
        let classes = "Software\\Classes";
        for (ext, prog, label) in [(".halcyontheme", "Halcyon.Theme", "Thème Halcyon"), (".halcyonplugin", "Halcyon.Plugin", "Extension Halcyon")] {
            let (k, _) = r.create_subkey(format!("{classes}\\{ext}")).map_err(|e| e.to_string())?;
            set(&k, "", prog)?;
            let (k, _) = r.create_subkey(format!("{classes}\\{prog}")).map_err(|e| e.to_string())?;
            set(&k, "", label)?;
            let (k, _) = r.create_subkey(format!("{classes}\\{prog}\\DefaultIcon")).map_err(|e| e.to_string())?;
            set(&k, "", &format!("{},0", exe.display()))?;
            let (k, _) = r.create_subkey(format!("{classes}\\{prog}\\shell\\open\\command")).map_err(|e| e.to_string())?;
            set(&k, "", &format!("{} \"%1\"", q(&exe)))?;
        }
        // Identité des notifications (AppUserModelID) pour une application non empaquetée.
        let (k, _) = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey(format!("Software\\Classes\\AppUserModelId\\{AUMID}"))
            .map_err(|e| e.to_string())?;
        set(&k, "DisplayName", "Halcyon")?;
        let ico = dir.join("halcyon.ico");
        if ico.is_file() {
            set(&k, "IconUri", &ico.display().to_string())?;
        }
        if autostart {
            let (k, _) = RegKey::predef(HKEY_CURRENT_USER)
                .create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
                .map_err(|e| e.to_string())?;
            set(&k, "Halcyon", &format!("{} --minimized", q(&exe)))?;
        }
        Ok(created)
    }

    /// Retire les clés créées par l'installation (et celles de l'ancien installateur).
    pub fn unregister(all_users: bool) {
        let r = root(all_users);
        let _ = r.delete_subkey_all(UNINSTALL_KEY);
        let _ = r.delete_subkey_all(INNO_KEY);
        for k in ["Software\\Classes\\Halcyon.Theme", "Software\\Classes\\Halcyon.Plugin"] {
            let _ = r.delete_subkey_all(k);
        }
        for ext in [".halcyontheme", ".halcyonplugin"] {
            if let Ok(k) = r.open_subkey(format!("Software\\Classes\\{ext}")) {
                if k.get_value::<String, _>("").is_ok_and(|v| v.starts_with("Halcyon.")) {
                    let _ = r.delete_subkey_all(format!("Software\\Classes\\{ext}"));
                }
            }
        }
        let cu = RegKey::predef(HKEY_CURRENT_USER);
        let _ = cu.delete_subkey_all(format!("Software\\Classes\\AppUserModelId\\{AUMID}"));
        if let Ok(k) = cu.open_subkey_with_flags("Software\\Microsoft\\Windows\\CurrentVersion\\Run", KEY_ALL_ACCESS) {
            let _ = k.delete_value("Halcyon");
        }
    }

    /// Installation existante déclarée dans le registre : (dossier, version, Inno Setup ?).
    pub fn registered() -> Option<(PathBuf, Option<String>, bool, bool)> {
        for all in [false, true] {
            for (key, inno) in [(UNINSTALL_KEY, false), (INNO_KEY, true)] {
                if let Ok(k) = root(all).open_subkey(key) {
                    if let Ok(loc) = k.get_value::<String, _>("InstallLocation") {
                        let v = k.get_value::<String, _>("DisplayVersion").ok();
                        return Some((PathBuf::from(loc.trim_end_matches('\\')), v, inno, all));
                    }
                }
            }
        }
        None
    }

    /// Le processus a-t-il les droits d'administrateur (écriture possible dans Program Files) ?
    pub fn elevated() -> bool {
        let Some(pf) = std::env::var_os("ProgramFiles") else { return false };
        let probe = PathBuf::from(pf).join(format!(".halcyon-probe-{}", std::process::id()));
        let ok = std::fs::write(&probe, b"").is_ok();
        let _ = std::fs::remove_file(&probe);
        ok
    }
}
