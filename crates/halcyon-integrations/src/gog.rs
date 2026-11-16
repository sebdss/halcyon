//! GOG (1.0.0) : jeux **installés**, sans authentification ni accès au compte.
//!
//! - **Heroic** (Linux, Windows ; natif ou Flatpak) : `gog_store/installed.json` (jeux
//!   installés) et le cache de bibliothèque (`store_cache/gog_library.json`, ou
//!   `gog_store/library.json` des anciennes versions) pour les titres et visuels. Lancement
//!   par `heroic://launch?appName=…&runner=gog` : Heroic applique ses réglages (Wine, Proton).
//! - **GOG Galaxy** (Windows) : registre `HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\<id>`
//!   (nom, dossier, exécutable, arguments, DLC). Les jeux GOG étant sans DRM, l'exécutable
//!   est lancé directement, avec les arguments déclarés par GOG.
//! - Tout dossier contenant un `goggame-<id>.info` (installateur hors ligne) est déjà vu par
//!   l'analyse des dossiers « standalone ».

use crate::{io_err, read_text};
use halcyon_core::{DiscoveredGame, GameScanner, GameSource, ScanContext, ScanReport};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct GogScanner {
    /// Dossier de configuration Heroic choisi dans les paramètres (sinon détection).
    heroic_override: Option<PathBuf>,
}

impl GogScanner {
    pub fn new(heroic_override: Option<PathBuf>) -> Self {
        Self { heroic_override }
    }

    fn roots(&self) -> Vec<PathBuf> {
        match &self.heroic_override {
            Some(p) if p.join("gog_store/installed.json").is_file() => vec![p.clone()],
            _ => heroic_roots(),
        }
    }

    /// Emplacement affiché dans les paramètres.
    pub fn location(&self) -> Option<PathBuf> {
        self.roots().into_iter().next().or_else(galaxy_location)
    }
}

/// Dossiers de configuration Heroic connus.
pub fn heroic_roots() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(cfg) = dirs::config_dir() {
        v.push(cfg.join("heroic"));
    }
    if let Some(home) = dirs::home_dir() {
        v.push(home.join(".var/app/com.heroicgameslauncher.hgl/config/heroic"));
    }
    v.into_iter().filter(|r| r.join("gog_store/installed.json").is_file()).collect()
}

#[cfg(windows)]
fn galaxy_location() -> Option<PathBuf> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    let k = winreg::RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\WOW6432Node\GOG.com\Games").ok()?;
    k.enum_keys().next().is_some().then(|| PathBuf::from(r"HKLM\SOFTWARE\WOW6432Node\GOG.com\Games"))
}

#[cfg(not(windows))]
fn galaxy_location() -> Option<PathBuf> {
    None
}

impl GameScanner for GogScanner {
    fn source(&self) -> GameSource {
        GameSource::Gog
    }

    fn scan(&self, ctx: &ScanContext) -> halcyon_core::Result<ScanReport> {
        let roots = self.roots();
        if let Some(p) = self.heroic_override.as_ref().filter(|p| !p.join("gog_store/installed.json").is_file()) {
            log::warn!("GOG : {} n'est pas un dossier de configuration Heroic, détection automatique", p.display());
        }
        let galaxy = galaxy_games();
        if roots.is_empty() && galaxy.is_none() {
            return Err(io_err("ni Heroic (GOG) ni GOG Galaxy n'ont été trouvés"));
        }
        let mut report = ScanReport { complete: true, ..Default::default() };
        let mut seen = std::collections::HashSet::new();
        for root in &roots {
            ctx.progress("GOG : lecture de Heroic…");
            match scan_heroic(root) {
                Ok(games) => report.games.extend(games.into_iter().filter(|g| seen.insert(g.source_key.clone()))),
                Err(e) => report.warnings.push(format!("{} : {e}", root.display())),
            }
        }
        if let Some(games) = galaxy {
            ctx.progress("GOG : lecture de GOG Galaxy…");
            report.games.extend(games.into_iter().filter(|g| seen.insert(g.source_key.clone())));
        }
        log::info!("GOG : {} jeux trouvés", report.games.len());
        Ok(report)
    }
}

/// Jeux installés selon Heroic.
pub fn scan_heroic(root: &Path) -> Result<Vec<DiscoveredGame>, String> {
    let text = read_text(&root.join("gog_store/installed.json")).map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let list = v.get("installed").and_then(|l| l.as_array()).or_else(|| v.as_array()).cloned().unwrap_or_default();
    let library = read_library(root);
    let mut out = Vec::new();
    for item in list {
        let s = |k: &str| item.get(k).and_then(|x| x.as_str()).map(str::trim).filter(|x| !x.is_empty()).map(str::to_string);
        let Some(app) = s("appName") else { continue };
        if item.get("is_dlc").and_then(|d| d.as_bool()).unwrap_or(false) {
            continue;
        }
        let meta = library.get(&app);
        let m = |k: &str| meta.and_then(|m| m.get(k)).and_then(|x| x.as_str()).filter(|x| !x.is_empty()).map(str::to_string);
        let title = m("title").unwrap_or_else(|| app.clone());
        let mut g = DiscoveredGame::new(app.clone(), title);
        let install = s("install_path").map(PathBuf::from);
        g.installed = install.as_ref().is_some_and(|p| p.is_dir());
        if let (Some(dir), Some(exe)) = (&install, s("executable")) {
            let p = PathBuf::from(&exe);
            g.executable = Some(if p.is_absolute() { p } else { dir.join(exe) });
        }
        g.install_dir = install;
        g.version = s("version");
        g.size_bytes = item.get("install_size").and_then(parse_size);
        g.launch_uri = Some(format!("heroic://launch?appName={app}&runner=gog"));
        g.artwork.cover_url = m("art_square");
        g.artwork.banner_url = m("art_cover").or_else(|| m("art_background"));
        g.artwork.logo_url = m("art_logo");
        g.developer = m("developer");
        g.description = m("description").or_else(|| meta.and_then(|m| m.pointer("/extra/about/shortDescription")).and_then(|x| x.as_str()).map(str::to_string));
        out.push(g);
    }
    Ok(out)
}

/// Cache de bibliothèque Heroic (titres, visuels), formats récent et ancien.
fn read_library(root: &Path) -> HashMap<String, serde_json::Value> {
    let mut out = HashMap::new();
    for p in [root.join("store_cache/gog_library.json"), root.join("gog_store/library.json")] {
        let Ok(text) = read_text(&p) else { continue };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
        let list = v.get("games").or_else(|| v.get("library")).and_then(|l| l.as_array()).cloned().unwrap_or_default();
        for item in list {
            if let Some(id) = item.get("app_name").and_then(|x| x.as_str()) {
                out.entry(id.to_string()).or_insert(item);
            }
        }
    }
    out
}

/// « 1.2 GiB », « 850 MiB » ou un nombre d'octets.
fn parse_size(v: &serde_json::Value) -> Option<u64> {
    if let Some(n) = v.as_u64() {
        return (n > 0).then_some(n);
    }
    let s = v.as_str()?.trim();
    let (num, unit) = s.split_once(' ').unwrap_or((s, "B"));
    let n: f64 = num.replace(',', ".").parse().ok()?;
    let mult = match unit.to_ascii_lowercase().as_str() {
        "b" => 1.0,
        "kib" | "kb" => 1024.0,
        "mib" | "mb" => 1024.0 * 1024.0,
        "gib" | "gb" => 1024.0 * 1024.0 * 1024.0,
        "tib" | "tb" => 1024f64.powi(4),
        _ => return None,
    };
    Some((n * mult) as u64).filter(|&b| b > 0)
}

/// Une entrée GOG Galaxy (registre), indépendante du système pour les tests.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GalaxyEntry {
    pub id: String,
    pub name: String,
    pub path: String,
    pub exe: String,
    pub args: String,
    pub working_dir: String,
    /// Non vide pour un DLC (identifiant du jeu parent).
    pub depends_on: String,
}

pub fn galaxy_to_game(e: GalaxyEntry) -> Option<DiscoveredGame> {
    if !e.depends_on.trim().is_empty() || e.name.trim().is_empty() {
        return None;
    }
    let mut g = DiscoveredGame::new(e.id.clone(), e.name.trim());
    let dir = PathBuf::from(e.path.trim());
    g.installed = dir.is_dir();
    if !e.exe.trim().is_empty() {
        let exe = PathBuf::from(e.exe.trim());
        g.executable = Some(if exe.is_absolute() { exe } else { dir.join(exe) });
    }
    g.arguments = e.args.trim().to_string();
    g.working_dir = Some(if e.working_dir.trim().is_empty() { dir.clone() } else { PathBuf::from(e.working_dir.trim()) });
    g.install_dir = Some(dir);
    Some(g)
}

#[cfg(windows)]
fn galaxy_games() -> Option<Vec<DiscoveredGame>> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    let root = winreg::RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\WOW6432Node\GOG.com\Games").ok()?;
    let mut out = Vec::new();
    for id in root.enum_keys().flatten() {
        let Ok(k) = root.open_subkey(&id) else { continue };
        let get = |n: &str| k.get_value::<String, _>(n).unwrap_or_default();
        let gid = get("gameID");
        let e = GalaxyEntry { id: if gid.is_empty() { id.clone() } else { gid }, name: get("gameName"), path: get("path"), exe: get("exe"), args: get("launchParam"), working_dir: get("workingDir"), depends_on: get("dependsOn") };
        out.extend(galaxy_to_game(e));
    }
    Some(out)
}

#[cfg(not(windows))]
fn galaxy_games() -> Option<Vec<DiscoveredGame>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_heroic_gog_installations() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("gog_store")).unwrap();
        std::fs::create_dir_all(root.join("store_cache")).unwrap();
        let game = root.join("Games/Witcher");
        std::fs::create_dir_all(&game).unwrap();
        let installed = serde_json::json!({"installed": [
            {"appName": "1207664663", "platform": "windows", "executable": "bin/witcher3.exe", "install_path": game, "install_size": "48.5 GiB", "is_dlc": false, "version": "4.04"},
            {"appName": "999", "platform": "windows", "install_path": "/nope", "is_dlc": true},
            {"appName": "42", "platform": "linux", "install_path": "/missing", "install_size": "850 MiB"}
        ]});
        std::fs::write(root.join("gog_store/installed.json"), installed.to_string()).unwrap();
        let lib = serde_json::json!({"games": [{"app_name": "1207664663", "title": "The Witcher 3", "art_square": "https://img/sq.jpg", "art_cover": "https://img/cov.jpg", "developer": "CD PROJEKT RED"}]});
        std::fs::write(root.join("store_cache/gog_library.json"), lib.to_string()).unwrap();

        let games = scan_heroic(root).unwrap();
        assert_eq!(games.len(), 2, "DLC ignoré");
        let w = &games[0];
        assert_eq!(w.name, "The Witcher 3");
        assert!(w.installed);
        assert_eq!(w.executable.as_deref(), Some(game.join("bin/witcher3.exe").as_path()));
        assert_eq!(w.launch_uri.as_deref(), Some("heroic://launch?appName=1207664663&runner=gog"));
        assert_eq!(w.size_bytes, Some((48.5 * 1024.0 * 1024.0 * 1024.0) as u64));
        assert_eq!(w.artwork.cover_url.as_deref(), Some("https://img/sq.jpg"));
        assert_eq!(w.developer.as_deref(), Some("CD PROJEKT RED"));
        let other = &games[1];
        assert_eq!(other.name, "42", "sans cache : identifiant affiché");
        assert!(!other.installed);
        assert_eq!(other.size_bytes, Some(850 * 1024 * 1024));
    }

    #[test]
    fn galaxy_entries_skip_dlc_and_resolve_paths() {
        let dir = tempfile::tempdir().unwrap();
        let e = GalaxyEntry { id: "1".into(), name: "Hollow".into(), path: dir.path().display().to_string(), exe: "game.exe".into(), args: "-skipintro".into(), ..Default::default() };
        let g = galaxy_to_game(e.clone()).unwrap();
        assert!(g.installed);
        assert_eq!(g.executable.unwrap(), dir.path().join("game.exe"));
        assert_eq!(g.arguments, "-skipintro");
        assert_eq!(g.working_dir.unwrap(), dir.path());
        assert!(galaxy_to_game(GalaxyEntry { depends_on: "1".into(), ..e }).is_none());
    }
}
