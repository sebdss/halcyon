//! Source Steam : jeux installés, visuels locaux, temps de jeu.
//!
//! Fichiers lus (tous locaux, aucune connexion requise) :
//! - `steamapps/libraryfolders.vdf` : bibliothèques Steam (plusieurs disques) ;
//! - `steamapps/appmanifest_<appid>.acf` : un manifeste par jeu installé ;
//! - `appcache/librarycache/` : jaquettes déjà téléchargées par le client ;
//! - `config/loginusers.vdf` + `userdata/<id>/config/localconfig.vdf` : temps de jeu.
//!
//! Lancement : `steam://rungameid/<appid>`. Le client Steam doit tourner (DRM,
//! Steamworks) ; il démarre en arrière-plan sans ouvrir sa bibliothèque si besoin.

use crate::vdf::Vdf;
use crate::{io_err, read_text};
use halcyon_core::{Artwork, DiscoveredGame, GameScanner, GameSource, ScanContext, ScanReport, UpdateInfo, UpdateState};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Applications Steam qui ne sont pas des jeux (redistribuables, runtimes Linux, Proton…).
const NON_GAME_APPIDS: &[u64] = &[228980, 1070560, 1391110, 1628350, 1826330, 2180100, 1493710];
const NON_GAME_PREFIXES: &[&str] = &["Proton ", "Steam Linux Runtime", "Steamworks Common", "Steamworks Shared"];

/// Décalage entre un SteamID64 et l'identifiant de compte utilisé dans `userdata/`.
const STEAMID64_BASE: u64 = 76_561_197_960_265_728;

pub struct SteamScanner {
    root_override: Option<PathBuf>,
    /// API Web (clé personnelle) : ajoute les jeux possédés non installés.
    web: Option<crate::steam_web::SteamWeb>,
}

impl SteamScanner {
    pub fn new(root_override: Option<PathBuf>) -> Self {
        Self { root_override, web: None }
    }

    pub fn with_web(mut self, web: Option<crate::steam_web::SteamWeb>) -> Self {
        self.web = web;
        self
    }

    /// Dossier d'installation de Steam (forcé ou détecté).
    pub fn root(&self) -> Option<PathBuf> {
        self.root_override.clone().filter(|p| p.join("steamapps").is_dir()).or_else(detect_root)
    }
}

/// Détection automatique du dossier Steam.
pub fn detect_root() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    #[cfg(windows)]
    {
        use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
        use winreg::RegKey;
        if let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Valve\Steam") {
            if let Ok(p) = k.get_value::<String, _>("SteamPath") {
                candidates.push(PathBuf::from(p.replace('/', "\\")));
            }
        }
        if let Ok(k) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\WOW6432Node\Valve\Steam") {
            if let Ok(p) = k.get_value::<String, _>("InstallPath") {
                candidates.push(PathBuf::from(p));
            }
        }
        candidates.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));
        candidates.push(PathBuf::from(r"C:\Program Files\Steam"));
    }
    #[cfg(not(windows))]
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        candidates.push(home.join(".steam/steam"));
        candidates.push(home.join(".local/share/Steam"));
        candidates.push(home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
    }
    candidates.into_iter().find(|p| p.join("steamapps").is_dir())
}

impl GameScanner for SteamScanner {
    fn source(&self) -> GameSource {
        GameSource::Steam
    }

    fn scan(&self, ctx: &ScanContext) -> halcyon_core::Result<ScanReport> {
        let root = self.root().ok_or_else(|| io_err(&tr!("Steam n'a pas été trouvé sur cet ordinateur")))?;
        ctx.progress(&tr!("Steam : lecture des bibliothèques…"));
        let mut report = ScanReport { complete: true, ..Default::default() };

        let libraries = library_folders(&root, &mut report.warnings);
        let stats = read_user_stats(&root);

        for lib in libraries {
            if ctx.is_cancelled() {
                report.complete = false;
                break;
            }
            let steamapps = lib.join("steamapps");
            let entries = match std::fs::read_dir(&steamapps) {
                Ok(e) => e,
                Err(e) => {
                    // Disque débranché : on ne peut pas conclure que ses jeux sont désinstallés.
                    report.warnings.push(tr!("bibliothèque Steam inaccessible {lib} : {e}", lib = lib.display(), e = e));
                    report.complete = false;
                    continue;
                }
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
                    continue;
                }
                match parse_manifest(&entry.path(), &steamapps) {
                    Ok(Some(mut game)) => {
                        let appid: u64 = game.source_key.parse().unwrap_or(0);
                        game.artwork = artwork_for(&root, appid);
                        if let Some(s) = stats.get(&appid) {
                            game.playtime_secs = s.playtime_minutes.map(|m| m * 60);
                            game.last_played = s.last_played.or(game.last_played);
                        }
                        report.games.push(game);
                    }
                    Ok(None) => {}
                    Err(e) => report.warnings.push(format!("{name} : {e}")),
                }
            }
        }
        if let Some(web) = &self.web {
            if !ctx.is_cancelled() {
                ctx.progress(&tr!("Steam : jeux possédés (en ligne)…"));
                let dl = crate::artwork::Downloader::new();
                match web.owned_games(&dl) {
                    Ok(owned) => merge_owned(&mut report.games, owned, &root),
                    Err(e) => report.warnings.push(tr!("jeux possédés indisponibles : {e}", e = e)),
                }
            }
        }
        log::info!("Steam : {} jeux trouvés ({} avertissements)", report.games.len(), report.warnings.len());
        Ok(report)
    }
}

/// Ajoute les jeux possédés absents du disque, et complète le temps de jeu des autres.
fn merge_owned(games: &mut Vec<DiscoveredGame>, owned: Vec<crate::steam_web::OwnedGame>, root: &Path) {
    for o in owned {
        let key = o.appid.to_string();
        if let Some(g) = games.iter_mut().find(|g| g.source_key == key) {
            g.playtime_secs = g.playtime_secs.max(Some(o.playtime_minutes * 60));
            g.last_played = g.last_played.max(o.last_played);
            continue;
        }
        if NON_GAME_PREFIXES.iter().any(|p| o.name.starts_with(p)) {
            continue;
        }
        let mut g = DiscoveredGame::new(key, o.name);
        g.installed = false;
        g.launch_uri = Some(format!("steam://rungameid/{}", o.appid));
        g.artwork = artwork_for(root, o.appid);
        g.playtime_secs = Some(o.playtime_minutes * 60).filter(|&s| s > 0);
        g.last_played = o.last_played;
        games.push(g);
    }
}

/// Liste des bibliothèques Steam (le dossier racine est toujours inclus).
fn library_folders(root: &Path, warnings: &mut Vec<String>) -> Vec<PathBuf> {
    let mut out = vec![root.to_path_buf()];
    let path = root.join("steamapps").join("libraryfolders.vdf");
    match read_text(&path).map_err(|e| e.to_string()).and_then(|t| Vdf::parse(&t).map_err(|e| e.to_string())) {
        Ok(v) => {
            if let Some(lf) = v.get("libraryfolders") {
                for (_, entry) in lf.entries() {
                    // Ancien format : "1" "D:\\SteamLibrary" ; nouveau : "1" { "path" "…" }
                    let p = entry.as_str().or_else(|| entry.get_str("path"));
                    if let Some(p) = p {
                        let p = PathBuf::from(p);
                        if !out.iter().any(|o| halcyon_core::util::path_key(o) == halcyon_core::util::path_key(&p)) {
                            out.push(p);
                        }
                    }
                }
            }
        }
        Err(e) if path.exists() => warnings.push(tr!("libraryfolders.vdf : {e}", e = e)),
        Err(_) => {}
    }
    out
}

fn parse_manifest(path: &Path, steamapps: &Path) -> Result<Option<DiscoveredGame>, String> {
    let text = read_text(path).map_err(|e| e.to_string())?;
    let v = Vdf::parse(&text).map_err(|e| e.to_string())?;
    let app = v.get("AppState").ok_or(tr!("section AppState absente"))?;
    let appid = app.get_u64("appid").ok_or(tr!("appid absent"))?;
    let name = app.get_str("name").map(str::trim).filter(|n| !n.is_empty()).ok_or(tr!("nom absent"))?;
    if NON_GAME_APPIDS.contains(&appid) || NON_GAME_PREFIXES.iter().any(|p| name.starts_with(p)) {
        return Ok(None);
    }
    // StateFlags : bit 2 (4) = entièrement installé.
    let flags = app.get_u64("StateFlags").unwrap_or(0);
    let install_dir = app.get_str("installdir").map(|d| steamapps.join("common").join(d));
    let mut g = DiscoveredGame::new(appid.to_string(), name);
    g.installed = flags & 4 != 0 && install_dir.as_ref().is_some_and(|d| d.is_dir());
    g.install_dir = install_dir;
    g.launch_uri = Some(format!("steam://rungameid/{appid}"));
    g.size_bytes = app.get_u64("SizeOnDisk").filter(|&s| s > 0);
    g.last_played = app.get_u64("LastPlayed").filter(|&t| t > 0).map(|t| t as i64);
    g.version = app.get_u64("buildid").filter(|&b| b > 0).map(|b| format!("build {b}"));
    g.update = Some(update_info(app));
    Ok(Some(g))
}

/// Indicateurs `StateFlags` des manifestes Steam (EAppState).
mod flags {
    pub const UPDATE_REQUIRED: u64 = 2;
    pub const UPDATE_RUNNING: u64 = 256;
    pub const UPDATE_PAUSED: u64 = 512;
    pub const UPDATE_STARTED: u64 = 1024;
    pub const DOWNLOADING: u64 = 1 << 20;
    pub const STAGING: u64 = 1 << 21;
    pub const COMMITTING: u64 = 1 << 22;
}

/// État de mise à jour d'après le manifeste : uniquement ce que Steam y écrit.
fn update_info(app: &Vdf) -> UpdateInfo {
    let f = app.get_u64("StateFlags").unwrap_or(0);
    let build = app.get_u64("buildid").unwrap_or(0);
    let target = app.get_u64("TargetBuildID").unwrap_or(0);
    let total = app.get_u64("BytesToDownload").unwrap_or(0);
    let downloaded = app.get_u64("BytesDownloaded").unwrap_or(0).min(total);
    let state = if f & flags::UPDATE_PAUSED != 0 {
        UpdateState::Paused
    } else if f & (flags::UPDATE_RUNNING | flags::UPDATE_STARTED | flags::DOWNLOADING | flags::STAGING | flags::COMMITTING) != 0 {
        UpdateState::Downloading
    } else if f & flags::UPDATE_REQUIRED != 0 || (target != 0 && build != 0 && target != build) {
        UpdateState::Available
    } else {
        UpdateState::None
    };
    if state == UpdateState::None {
        UpdateInfo::default()
    } else {
        UpdateInfo { state, downloaded, total }
    }
}

/// Relit le manifeste d'un seul jeu (suivi d'un téléchargement en cours).
pub fn read_app(root: &Path, appid: &str) -> Option<DiscoveredGame> {
    let mut warnings = Vec::new();
    library_folders(root, &mut warnings).into_iter().find_map(|lib| {
        let steamapps = lib.join("steamapps");
        let path = steamapps.join(format!("appmanifest_{appid}.acf"));
        parse_manifest(&path, &steamapps).ok().flatten()
    })
}

/// Jeu actuellement lancé par Steam, d'après le client (`RunningAppID`).
pub fn running_app_id(root: &Path) -> Option<u64> {
    #[cfg(windows)]
    {
        let _ = root;
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;
        let k = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Valve\Steam").ok()?;
        let id: u32 = k.get_value("RunningAppID").ok()?;
        (id != 0).then_some(id as u64)
    }
    #[cfg(not(windows))]
    {
        let mut candidates = vec![root.join("registry.vdf")];
        if let Some(parent) = root.parent() {
            candidates.push(parent.join("registry.vdf"));
        }
        if let Some(home) = dirs::home_dir() {
            candidates.push(home.join(".steam/registry.vdf"));
        }
        let text = candidates.iter().find_map(|p| read_text(p).ok())?;
        running_from_registry(&text)
    }
}

#[cfg_attr(windows, allow(dead_code))]
fn running_from_registry(text: &str) -> Option<u64> {
    let v = Vdf::parse(text).ok()?;
    let steam = v.path(&["Registry", "HKCU", "Software", "Valve", "Steam"])?;
    steam.get_u64("RunningAppID").filter(|&id| id != 0)
}

/// Visuels : fichiers déjà en cache dans le client Steam, et URL du CDN en secours.
fn artwork_for(root: &Path, appid: u64) -> Artwork {
    let cache = root.join("appcache").join("librarycache");
    let find = |names: &[&str]| find_cached(&cache, appid, names);
    let cdn = format!("https://shared.cloudflare.steamstatic.com/store_item_assets/steam/apps/{appid}");
    Artwork {
        cover: find(&["library_600x900_2x.jpg", "library_600x900.jpg", "library_capsule.jpg"]),
        banner: find(&["library_hero.jpg", "library_hero_2x.jpg"]),
        logo: find(&["logo.png", "logo_2x.png"]),
        cover_url: Some(format!("{cdn}/library_600x900_2x.jpg")),
        banner_url: Some(format!("{cdn}/library_hero.jpg")),
        logo_url: Some(format!("{cdn}/logo.png")),
    }
}

/// Cherche un visuel dans les deux dispositions du cache Steam :
/// ancienne `librarycache/<appid>_<nom>`, récente `librarycache/<appid>/[<hash>/]<nom>`.
fn find_cached(cache: &Path, appid: u64, names: &[&str]) -> Option<PathBuf> {
    let dir = cache.join(appid.to_string());
    for name in names {
        let flat = cache.join(format!("{appid}_{name}"));
        if flat.is_file() {
            return Some(flat);
        }
        let direct = dir.join(name);
        if direct.is_file() {
            return Some(direct);
        }
    }
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for sub in entries.flatten().filter(|e| e.path().is_dir()) {
            for name in names {
                let p = sub.path().join(name);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    None
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct AppStats {
    playtime_minutes: Option<u64>,
    last_played: Option<i64>,
}

/// Temps de jeu et dernier lancement du compte Steam le plus récemment connecté.
fn read_user_stats(root: &Path) -> HashMap<u64, AppStats> {
    let Some(account) = most_recent_account(root) else { return HashMap::new() };
    let path = root.join("userdata").join(account.to_string()).join("config").join("localconfig.vdf");
    let Ok(text) = read_text(&path) else { return HashMap::new() };
    match Vdf::parse(&text) {
        Ok(v) => parse_localconfig_apps(&v),
        Err(e) => {
            log::warn!("localconfig.vdf illisible : {e}");
            HashMap::new()
        }
    }
}

fn parse_localconfig_apps(v: &Vdf) -> HashMap<u64, AppStats> {
    let mut out = HashMap::new();
    let Some(apps) = v.path(&["UserLocalConfigStore", "Software", "Valve", "Steam", "apps"]) else { return out };
    for (id, data) in apps.entries() {
        let Ok(appid) = id.parse::<u64>() else { continue };
        let stats = AppStats {
            playtime_minutes: data.get_u64("Playtime").filter(|&m| m > 0),
            last_played: data.get_u64("LastPlayed").filter(|&t| t > 0).map(|t| t as i64),
        };
        if stats != AppStats::default() {
            out.insert(appid, stats);
        }
    }
    out
}

/// Capture d'écran prise avec l'overlay Steam (F12), lue dans `userdata/<compte>/760/remote`.
#[derive(Clone, Debug, PartialEq)]
pub struct Screenshot {
    pub appid: u64,
    pub path: PathBuf,
    /// Vignette produite par Steam (plus légère à décoder), si présente.
    pub thumbnail: Option<PathBuf>,
    pub modified: i64,
}

/// Captures Steam locales, les plus récentes d'abord. `appid` : filtre facultatif.
/// Lecture seule : Halcyon ne modifie ni ne déplace jamais ces fichiers.
pub fn screenshots(root: &Path, appid: Option<u64>, limit: usize) -> Vec<Screenshot> {
    let mut out = Vec::new();
    let Ok(users) = std::fs::read_dir(root.join("userdata")) else { return out };
    for user in users.flatten() {
        let remote = user.path().join("760").join("remote");
        let Ok(apps) = std::fs::read_dir(&remote) else { continue };
        for app in apps.flatten() {
            let Some(id) = app.file_name().to_str().and_then(|n| n.parse::<u64>().ok()) else { continue };
            if appid.is_some_and(|a| a != id) {
                continue;
            }
            let dir = app.path().join("screenshots");
            let Ok(files) = std::fs::read_dir(&dir) else { continue };
            for f in files.flatten() {
                let path = f.path();
                let is_image = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "jpg" | "jpeg" | "png"));
                if !is_image || !path.is_file() {
                    continue;
                }
                let modified = f
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs() as i64);
                let thumb = dir.join("thumbnails").join(f.file_name());
                out.push(Screenshot { appid: id, path, thumbnail: thumb.is_file().then_some(thumb), modified });
            }
        }
    }
    out.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.path.cmp(&b.path)));
    out.truncate(limit);
    out
}

/// SteamID64 du compte le plus récemment connecté sur cet ordinateur.
pub fn local_steam_id64(root: &Path) -> Option<u64> {
    most_recent_account(root).map(|a| a + STEAMID64_BASE)
}

/// Identifiant de compte (dossier `userdata/<id>`) de l'utilisateur le plus récent.
fn most_recent_account(root: &Path) -> Option<u64> {
    if let Ok(text) = read_text(&root.join("config").join("loginusers.vdf")) {
        if let Ok(v) = Vdf::parse(&text) {
            if let Some(users) = v.get("users") {
                let mut best: Option<(bool, u64, u64)> = None; // (MostRecent, Timestamp, id64)
                for (id, u) in users.entries() {
                    let Ok(id64) = id.parse::<u64>() else { continue };
                    let cand = (u.get_str("MostRecent") == Some("1"), u.get_u64("Timestamp").unwrap_or(0), id64);
                    if best.is_none_or(|b| (cand.0, cand.1) > (b.0, b.1)) {
                        best = Some(cand);
                    }
                }
                if let Some((_, _, id64)) = best {
                    return id64.checked_sub(STEAMID64_BASE);
                }
            }
        }
    }
    // Repli : le dossier userdata dont localconfig.vdf est le plus récent.
    std::fs::read_dir(root.join("userdata"))
        .ok()?
        .flatten()
        .filter_map(|e| {
            let id: u64 = e.file_name().to_string_lossy().parse().ok()?;
            let m = e.path().join("config/localconfig.vdf").metadata().ok()?.modified().ok()?;
            Some((m, id))
        })
        .max()
        .map(|(_, id)| id)
}

#[cfg(test)]
mod tests {
    #[test]
    fn screenshots_are_listed_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("userdata/123/760/remote");
        for (app, name) in [("620", "a.jpg"), ("620", "b.jpg"), ("440", "c.png")] {
            let d = base.join(app).join("screenshots");
            std::fs::create_dir_all(d.join("thumbnails")).unwrap();
            std::fs::write(d.join(name), b"x").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(1100 * u64::from(name == "c.png")));
        }
        std::fs::write(base.join("620/screenshots/thumbnails/a.jpg"), b"t").unwrap();
        std::fs::write(base.join("620/screenshots/notes.txt"), b"n").unwrap();
        let all = screenshots(dir.path(), None, 10);
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].appid, 440);
        let portal = screenshots(dir.path(), Some(620), 10);
        assert_eq!(portal.len(), 2);
        assert!(portal.iter().any(|s| s.thumbnail.is_some()));
        assert_eq!(screenshots(dir.path(), None, 1).len(), 1);
    }

    use super::*;
    use std::fs;

    /// Construit une fausse installation Steam conforme aux formats réels.
    fn fake_steam(dir: &Path) -> PathBuf {
        let root = dir.join("Steam");
        let lib2 = dir.join("SteamLibrary");
        fs::create_dir_all(root.join("steamapps/common/Portal 2")).unwrap();
        fs::create_dir_all(lib2.join("steamapps/common/Hades")).unwrap();
        fs::create_dir_all(root.join("appcache/librarycache/1145360/abc123")).unwrap();
        fs::create_dir_all(root.join("config")).unwrap();
        fs::create_dir_all(root.join("userdata/22202/config")).unwrap();
        fs::write(
            root.join("steamapps/libraryfolders.vdf"),
            format!(
                "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}",
                root.display().to_string().replace('\\', "\\\\"),
                lib2.display().to_string().replace('\\', "\\\\")
            ),
        )
        .unwrap();
        fs::write(
            root.join("steamapps/appmanifest_620.acf"),
            "\"AppState\"\n{\n\t\"appid\"\t\t\"620\"\n\t\"name\"\t\t\"Portal 2\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"installdir\"\t\t\"Portal 2\"\n\t\"SizeOnDisk\"\t\t\"12000000000\"\n}",
        )
        .unwrap();
        fs::write(
            root.join("steamapps/appmanifest_228980.acf"),
            "\"AppState\" { \"appid\" \"228980\" \"name\" \"Steamworks Common Redistributables\" \"StateFlags\" \"4\" }",
        )
        .unwrap();
        fs::write(
            lib2.join("steamapps/appmanifest_1145360.acf"),
            "\"AppState\" { \"appid\" \"1145360\" \"name\" \"Hades\" \"StateFlags\" \"4\" \"installdir\" \"Hades\" }",
        )
        .unwrap();
        fs::write(root.join("steamapps/appmanifest_999.acf"), "\"AppState\" { \"appid\" ").unwrap();
        fs::write(root.join("appcache/librarycache/620_library_600x900.jpg"), b"jpg").unwrap();
        fs::write(root.join("appcache/librarycache/1145360/abc123/library_600x900.jpg"), b"jpg").unwrap();
        fs::write(
            root.join("config/loginusers.vdf"),
            "\"users\" { \"76561197960287930\" { \"AccountName\" \"x\" \"MostRecent\" \"1\" \"Timestamp\" \"1700000000\" } }",
        )
        .unwrap();
        fs::write(
            root.join("userdata/22202/config/localconfig.vdf"),
            "\"UserLocalConfigStore\" { \"Software\" { \"Valve\" { \"Steam\" { \"apps\" { \"620\" { \"LastPlayed\" \"1700000123\" \"Playtime\" \"95\" } } } } } }",
        )
        .unwrap();
        root
    }

    #[test]
    fn scans_libraries_manifests_artwork_and_playtime() {
        let tmp = tempfile::tempdir().unwrap();
        let root = fake_steam(tmp.path());
        let report = SteamScanner::new(Some(root.clone())).scan(&ScanContext::silent()).unwrap();

        let mut names: Vec<&str> = report.games.iter().map(|g| g.name.as_str()).collect();
        names.sort();
        assert_eq!(names, ["Hades", "Portal 2"], "redistribuables exclus, deux bibliothèques lues");
        assert_eq!(report.warnings.len(), 1, "manifeste tronqué signalé sans bloquer");
        assert!(report.complete);

        let portal = report.games.iter().find(|g| g.source_key == "620").unwrap();
        assert!(portal.installed);
        assert_eq!(portal.launch_uri.as_deref(), Some("steam://rungameid/620"));
        assert_eq!(portal.playtime_secs, Some(95 * 60));
        assert_eq!(portal.last_played, Some(1_700_000_123));
        assert_eq!(portal.size_bytes, Some(12_000_000_000));
        assert!(portal.artwork.cover.as_ref().unwrap().ends_with("620_library_600x900.jpg"));

        let hades = report.games.iter().find(|g| g.source_key == "1145360").unwrap();
        assert!(hades.artwork.cover.as_ref().unwrap().ends_with("abc123/library_600x900.jpg"));
        assert!(hades.artwork.cover_url.as_ref().unwrap().contains("1145360"));
    }

    #[test]
    fn owned_games_complete_the_local_library() {
        use crate::steam_web::OwnedGame;
        let mut games = vec![DiscoveredGame::new("620", "Portal 2")];
        let owned = vec![
            OwnedGame { appid: 620, name: "Portal 2".into(), playtime_minutes: 300, last_played: Some(5) },
            OwnedGame { appid: 400, name: "Portal".into(), playtime_minutes: 0, last_played: None },
        ];
        merge_owned(&mut games, owned, Path::new("/nope"));
        assert_eq!(games.len(), 2);
        assert_eq!(games[0].playtime_secs, Some(18_000));
        assert!(!games[1].installed);
        assert_eq!(local_steam_id64(Path::new("/nope")), None);
    }

    #[test]
    fn update_states_and_running_app() {
        let parse = |t: &str| update_info(Vdf::parse(t).unwrap().get("AppState").unwrap());
        let up = parse(r#""AppState" { "StateFlags" "4" "buildid" "10" "TargetBuildID" "10" }"#);
        assert_eq!(up.state, UpdateState::None);
        let req = parse(r#""AppState" { "StateFlags" "6" "buildid" "10" "TargetBuildID" "12" "BytesToDownload" "500" }"#);
        assert_eq!((req.state, req.total), (UpdateState::Available, 500));
        let dl = parse(
            r#""AppState" { "StateFlags" "1048582" "BytesToDownload" "1000" "BytesDownloaded" "250" }"#,
        );
        assert_eq!((dl.state, dl.downloaded, dl.total), (UpdateState::Downloading, 250, 1000));
        let paused = parse(r#""AppState" { "StateFlags" "518" }"#);
        assert_eq!(paused.state, UpdateState::Paused);
        let reg = r#""Registry" { "HKCU" { "Software" { "Valve" { "Steam" { "RunningAppID" "620" } } } } }"#;
        assert_eq!(running_from_registry(reg), Some(620));
        assert_eq!(running_from_registry(&reg.replace("620", "0")), None);
    }

    #[test]
    fn missing_steam_is_an_error_not_an_empty_library() {
        let tmp = tempfile::tempdir().unwrap();
        let s = SteamScanner { root_override: Some(tmp.path().join("nope")), web: None };
        // La détection automatique peut trouver un vrai Steam sur la machine de dev :
        // on ne vérifie que le cas « dossier forcé invalide et aucun Steam ».
        if detect_root().is_none() {
            assert!(s.scan(&ScanContext::silent()).is_err());
        }
    }
}
