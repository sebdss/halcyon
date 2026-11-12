//! Source Epic Games : jeux installés via les manifestes du launcher Epic.
//!
//! - `…\EpicGamesLauncher\Data\Manifests\*.item` : un manifeste JSON par installation ;
//! - `…\EpicGamesLauncher\Data\Catalog\catcache.bin` : cache du catalogue (JSON en base64)
//!   contenant descriptions, développeurs et URL des visuels — lu hors ligne.
//!
//! Lancement : `com.epicgames.launcher://apps/<ns>%3A<item>%3A<app>?action=launch&silent=true`.
//! Le launcher Epic gère l'authentification et les DRM ; `silent=true` évite
//! l'affichage de son interface.
//!
//! Linux (et Windows sans launcher Epic) : jeux installés via **Heroic** ou
//! **Legendary** (`installed.json`), lancés par `heroic://launch?…` ou `legendary launch`.

use crate::{io_err, read_text};
use base64::Engine;
use halcyon_core::{DiscoveredGame, GameScanner, GameSource, ScanContext, ScanReport, UpdateInfo, UpdateState, LEGENDARY_URI};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct EpicScanner {
    manifests_override: Option<PathBuf>,
}

impl EpicScanner {
    pub fn new(manifests_override: Option<PathBuf>) -> Self {
        Self { manifests_override }
    }

    /// Dossier des manifestes : celui des paramètres s'il en contient, sinon la détection
    /// automatique (un dossier choisi par erreur ne doit pas masquer les jeux).
    pub fn manifests_dir(&self) -> Option<PathBuf> {
        self.manifests_override.clone().filter(|p| has_manifests(p)).or_else(detect_manifests_dir)
    }

    /// Dossier indiqué dans les paramètres mais ignoré (aucun manifeste `.item`).
    pub fn ignored_override(&self) -> Option<&Path> {
        self.manifests_override.as_deref().filter(|p| !p.as_os_str().is_empty() && !has_manifests(p))
    }

    /// Emplacement affiché dans les paramètres : launcher Epic, sinon Heroic/Legendary.
    pub fn location(&self) -> Option<PathBuf> {
        self.manifests_dir().or_else(|| legendary_configs().into_iter().next().map(|c| c.installed))
    }
}

/// Une configuration Legendary trouvée (autonome ou embarquée dans Heroic).
#[derive(Clone, Debug)]
pub struct LegendaryConfig {
    pub installed: PathBuf,
    /// Cache de bibliothèque Heroic (visuels, descriptions), si présent.
    pub heroic_library: Option<PathBuf>,
    pub heroic: bool,
}

/// Configurations Legendary connues (Heroic natif, Heroic Flatpak, Legendary seul).
pub fn legendary_configs() -> Vec<LegendaryConfig> {
    let mut roots: Vec<(PathBuf, bool)> = Vec::new();
    if let Some(cfg) = dirs::config_dir() {
        roots.push((cfg.join("heroic"), true));
        roots.push((cfg.join("legendary"), false));
    }
    if let Some(home) = dirs::home_dir() {
        roots.push((home.join(".var/app/com.heroicgameslauncher.hgl/config/heroic"), true));
        roots.push((home.join(".config/legendary"), false));
    }
    let mut out: Vec<LegendaryConfig> = Vec::new();
    for (root, heroic) in roots {
        let installed = if heroic { root.join("legendaryConfig/legendary/installed.json") } else { root.join("installed.json") };
        if installed.is_file() && !out.iter().any(|c| c.installed == installed) {
            let lib = root.join("store_cache/legendary_library.json");
            out.push(LegendaryConfig { installed, heroic_library: (heroic && lib.is_file()).then_some(lib), heroic });
        }
    }
    out
}

/// Le dossier contient-il au moins un manifeste Epic (`*.item`) ?
fn has_manifests(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|rd| rd.flatten().any(|e| e.path().extension().is_some_and(|x| x == "item")))
}

/// Détection automatique du dossier des manifestes Epic.
pub fn detect_manifests_dir() -> Option<PathBuf> {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut candidates: Vec<PathBuf> = Vec::new();
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_LOCAL_MACHINE;
        use winreg::RegKey;
        if let Ok(k) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\WOW6432Node\Epic Games\EpicGamesLauncher") {
            if let Ok(p) = k.get_value::<String, _>("AppDataPath") {
                candidates.push(PathBuf::from(p).join("Manifests"));
            }
        }
        let program_data = std::env::var_os("PROGRAMDATA").map(PathBuf::from).unwrap_or_else(|| r"C:\ProgramData".into());
        candidates.push(program_data.join(r"Epic\EpicGamesLauncher\Data\Manifests"));
    }
    candidates.into_iter().find(|p| p.is_dir())
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Manifest {
    display_name: Option<String>,
    app_name: Option<String>,
    catalog_namespace: Option<String>,
    catalog_item_id: Option<String>,
    install_location: Option<String>,
    launch_executable: Option<String>,
    main_game_app_name: Option<String>,
    #[serde(default)]
    app_categories: Vec<String>,
    #[serde(rename = "bIsIncompleteInstall", default)]
    incomplete: bool,
    app_version_string: Option<String>,
    install_size: Option<u64>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
struct CatalogItem {
    id: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    developer: Option<String>,
    #[serde(default)]
    key_images: Vec<KeyImage>,
}

#[derive(Deserialize, Clone)]
struct KeyImage {
    #[serde(rename = "type")]
    kind: String,
    url: String,
}

impl GameScanner for EpicScanner {
    fn source(&self) -> GameSource {
        GameSource::Epic
    }

    fn scan(&self, ctx: &ScanContext) -> halcyon_core::Result<ScanReport> {
        let dir = self.manifests_dir();
        let legendary = legendary_configs();
        if dir.is_none() && legendary.is_empty() {
            return Err(io_err("ni le launcher Epic Games, ni Heroic, ni Legendary n'ont été trouvés"));
        }
        let mut report = ScanReport { complete: true, ..Default::default() };
        for cfg in &legendary {
            ctx.progress(if cfg.heroic { "Epic Games : lecture de Heroic…" } else { "Epic Games : lecture de Legendary…" });
            match scan_legendary(cfg) {
                Ok(games) => report.games.extend(games),
                Err(e) => report.warnings.push(format!("{} : {e}", cfg.installed.display())),
            }
        }
        let Some(dir) = dir else {
            log::info!("Epic (Heroic/Legendary) : {} jeux trouvés", report.games.len());
            return Ok(report);
        };
        if let Some(p) = self.ignored_override() {
            report.warnings.push(format!(
                "{} ne contient aucun manifeste Epic : dossier détecté automatiquement utilisé ({})",
                p.display(),
                dir.display()
            ));
        }
        ctx.progress("Epic Games : lecture des manifestes…");
        let catalog = dir.parent().map(|data| read_catalog(&data.join("Catalog").join("catcache.bin"))).unwrap_or_default();

        let mut manifests = Vec::new();
        for entry in std::fs::read_dir(&dir)?.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("item") {
                continue;
            }
            match read_manifest(&path) {
                Ok(Some(m)) => manifests.push(m),
                Ok(None) => {}
                Err(e) => report.warnings.push(format!("{} : {e}", path.display())),
            }
        }
        // Plugins de l'Unreal Engine (Fab, Quixel Bridge…) : installés dans le dossier du
        // moteur, souvent sans catégorie. Ce ne sont pas des jeux.
        let engine_dirs: Vec<String> = manifests
            .iter()
            .filter(|m| m.app_categories.iter().any(|c| c.starts_with("engines")))
            .filter_map(|m| m.install_location.clone())
            .collect();
        for m in manifests {
            let is_game = m.app_categories.iter().any(|c| c == "games");
            let in_engine = m.install_location.as_ref().is_some_and(|l| engine_dirs.contains(l));
            if !is_game && in_engine {
                continue;
            }
            if let Some(g) = to_game(m, &catalog) {
                report.games.push(g);
            }
        }
        log::info!("Epic : {} jeux trouvés", report.games.len());
        Ok(report)
    }
}

fn read_manifest(path: &Path) -> Result<Option<Manifest>, String> {
    let text = read_text(path).map_err(|e| e.to_string())?;
    let m: Manifest = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(Some(m))
}

fn to_game(m: Manifest, catalog: &HashMap<String, CatalogItem>) -> Option<DiscoveredGame> {
    let app = m.app_name.filter(|a| !a.is_empty())?;
    // DLC : leur AppName diffère de celui du jeu principal. Moteurs Unreal : catégorie « engines ».
    // Le launcher écrit souvent `"MainGameAppName": ""` pour un jeu principal (Fortnite).
    if m.main_game_app_name.as_deref().is_some_and(|main| !main.is_empty() && main != app) {
        return None;
    }
    if !m.app_categories.is_empty() && !m.app_categories.iter().any(|c| c == "games") {
        return None;
    }
    let name = m.display_name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| app.clone());
    let mut g = DiscoveredGame::new(app.clone(), name.trim());
    let install = m.install_location.map(PathBuf::from);
    g.installed = !m.incomplete && install.as_ref().is_some_and(|p| p.is_dir());
    g.executable = match (&install, &m.launch_executable) {
        (Some(dir), Some(exe)) if !exe.is_empty() => Some(dir.join(exe)),
        _ => None,
    };
    g.install_dir = install;
    g.size_bytes = m.install_size.filter(|&s| s > 0);
    g.version = m.app_version_string.filter(|v| !v.is_empty());
    if let (Some(ns), Some(item)) = (&m.catalog_namespace, &m.catalog_item_id) {
        g.launch_uri = Some(format!("com.epicgames.launcher://apps/{ns}%3A{item}%3A{app}?action=launch&silent=true"));
        if let Some(c) = catalog.get(item) {
            g.description = c.description.clone().filter(|d| !d.trim().is_empty());
            g.developer = c.developer.clone().filter(|d| !d.trim().is_empty());
            let img = |kinds: &[&str]| {
                kinds.iter().find_map(|k| c.key_images.iter().find(|i| i.kind == *k).map(|i| i.url.clone()))
            };
            g.artwork.cover_url = img(&["DieselGameBoxTall", "OfferImageTall", "Thumbnail"]);
            g.artwork.banner_url = img(&["DieselGameBox", "OfferImageWide", "DieselStoreFrontWide"]);
            g.artwork.logo_url = img(&["DieselGameBoxLogo"]);
        }
    }
    Some(g)
}

/// Jeux installés selon Legendary (`installed.json`), enrichis du cache Heroic.
fn scan_legendary(cfg: &LegendaryConfig) -> Result<Vec<DiscoveredGame>, String> {
    let text = read_text(&cfg.installed).map_err(|e| e.to_string())?;
    let installed: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let library = cfg.heroic_library.as_ref().map(|p| read_heroic_library(p)).unwrap_or_default();
    // Versions les plus récentes connues de Legendary (cache rafraîchi par Heroic/Legendary).
    let latest = cfg.installed.parent().map(|d| read_assets(&d.join("assets.json"))).unwrap_or_default();
    let mut out = Vec::new();
    for (app, v) in installed {
        if v.get("is_dlc").and_then(|d| d.as_bool()).unwrap_or(false) {
            continue;
        }
        let title = v.get("title").and_then(|t| t.as_str()).unwrap_or(&app).trim().to_string();
        let mut g = DiscoveredGame::new(app.clone(), title);
        let install = v.get("install_path").and_then(|p| p.as_str()).map(PathBuf::from);
        g.installed = install.as_ref().is_some_and(|p| p.is_dir());
        if let (Some(dir), Some(exe)) = (&install, v.get("executable").and_then(|e| e.as_str())) {
            if !exe.is_empty() {
                g.executable = Some(dir.join(exe));
            }
        }
        g.install_dir = install;
        g.size_bytes = v.get("install_size").and_then(|s| s.as_u64()).filter(|&s| s > 0);
        g.version = v.get("version").and_then(|x| x.as_str()).filter(|x| !x.is_empty()).map(str::to_string);
        let platform = v.get("platform").and_then(|x| x.as_str()).unwrap_or("Windows");
        if let (Some(cur), Some(new)) = (&g.version, latest.get(&(app.clone(), platform.to_string()))) {
            g.update = Some(if cur != new {
                UpdateInfo { state: UpdateState::Available, ..Default::default() }
            } else {
                UpdateInfo::default()
            });
        }
        g.launch_uri = Some(if cfg.heroic {
            format!("heroic://launch?appName={app}&runner=legendary")
        } else {
            format!("{LEGENDARY_URI}{app}")
        });
        if let Some(meta) = library.get(&app) {
            let s = |k: &str| meta.get(k).and_then(|x| x.as_str()).filter(|x| !x.is_empty()).map(str::to_string);
            g.artwork.cover_url = s("art_square");
            g.artwork.banner_url = s("art_cover");
            g.artwork.logo_url = s("art_logo");
            g.developer = s("developer");
            g.description = s("description").or_else(|| {
                meta.pointer("/extra/about/shortDescription").and_then(|x| x.as_str()).map(str::to_string)
            });
        }
        out.push(g);
    }
    Ok(out)
}

/// `assets.json` de Legendary : dernière version publiée par jeu et plateforme.
fn read_assets(path: &Path) -> HashMap<(String, String), String> {
    let Ok(text) = read_text(path) else { return HashMap::new() };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return HashMap::new() };
    let mut out = HashMap::new();
    let mut add = |platform: &str, list: &serde_json::Value| {
        for a in list.as_array().into_iter().flatten() {
            if let (Some(app), Some(build)) =
                (a.get("app_name").and_then(|x| x.as_str()), a.get("build_version").and_then(|x| x.as_str()))
            {
                out.insert((app.to_string(), platform.to_string()), build.to_string());
            }
        }
    };
    match &v {
        serde_json::Value::Object(map) => map.iter().for_each(|(platform, list)| add(platform, list)),
        list @ serde_json::Value::Array(_) => add("Windows", list),
        _ => {}
    }
    out
}

fn read_heroic_library(path: &Path) -> HashMap<String, serde_json::Value> {
    let Ok(text) = read_text(path) else { return HashMap::new() };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return HashMap::new() };
    let list = v.get("library").and_then(|l| l.as_array()).or_else(|| v.as_array()).cloned().unwrap_or_default();
    list.into_iter()
        .filter_map(|item| Some((item.get("app_name")?.as_str()?.to_string(), item)))
        .collect()
}

/// Lit le cache du catalogue Epic (`catcache.bin` : JSON encodé en base64).
fn read_catalog(path: &Path) -> HashMap<String, CatalogItem> {
    let Ok(raw) = std::fs::read(path) else { return HashMap::new() };
    let raw: Vec<u8> = raw.into_iter().filter(|b| !b.is_ascii_whitespace()).collect();
    let decoded = match base64::engine::general_purpose::STANDARD.decode(&raw) {
        Ok(d) => d,
        Err(e) => {
            log::warn!("catcache.bin illisible : {e}");
            return HashMap::new();
        }
    };
    match serde_json::from_slice::<Vec<CatalogItem>>(&decoded) {
        Ok(items) => items.into_iter().map(|i| (i.id.clone(), i)).collect(),
        Err(e) => {
            log::warn!("catcache.bin : JSON inattendu ({e})");
            HashMap::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scans_manifests_skips_dlc_and_reads_catalog() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("Data");
        let manifests = data.join("Manifests");
        let game_dir = tmp.path().join("Games/Celeste");
        fs::create_dir_all(&manifests).unwrap();
        fs::create_dir_all(data.join("Catalog")).unwrap();
        fs::create_dir_all(&game_dir).unwrap();
        let loc = game_dir.display().to_string().replace('\\', "\\\\");

        fs::write(
            manifests.join("A.item"),
            format!(
                r#"{{"FormatVersion":0,"DisplayName":"Celeste","AppName":"Salt","MainGameAppName":"Salt",
                "CatalogNamespace":"ns1","CatalogItemId":"item1","InstallLocation":"{loc}",
                "LaunchExecutable":"Celeste.exe","AppCategories":["public","games","applications"],
                "bIsIncompleteInstall":false,"InstallSize":1234}}"#
            ),
        )
        .unwrap();
        fs::write(
            manifests.join("B.item"),
            r#"{"DisplayName":"Celeste DLC","AppName":"SaltDLC","MainGameAppName":"Salt","AppCategories":["addons"]}"#,
        )
        .unwrap();
        fs::write(
            manifests.join("C.item"),
            r#"{"DisplayName":"Unreal Engine","AppName":"UE_5.4","MainGameAppName":"UE_5.4","AppCategories":["engines"]}"#,
        )
        .unwrap();
        fs::write(manifests.join("broken.item"), "{").unwrap();

        let catalog = r#"[{"id":"item1","title":"Celeste","description":"Aidez Madeline.","developer":"Maddy Makes Games",
            "keyImages":[{"type":"DieselGameBoxTall","url":"https://img/tall.jpg"},{"type":"DieselGameBox","url":"https://img/wide.jpg"}]}]"#;
        fs::write(data.join("Catalog/catcache.bin"), base64::engine::general_purpose::STANDARD.encode(catalog)).unwrap();

        let report = EpicScanner::new(Some(manifests)).scan(&ScanContext::silent()).unwrap();
        let report_games: Vec<_> = report.games.iter().filter(|g| g.source_key == "Salt").collect();
        assert_eq!(report_games.len(), 1);
        assert_eq!(report.warnings.iter().filter(|w| w.contains("broken")).count(), 1);
        let g = report_games[0];
        assert_eq!(g.name, "Celeste");
        assert!(g.installed);
        assert_eq!(
            g.launch_uri.as_deref(),
            Some("com.epicgames.launcher://apps/ns1%3Aitem1%3ASalt?action=launch&silent=true")
        );
        assert_eq!(g.description.as_deref(), Some("Aidez Madeline."));
        assert_eq!(g.developer.as_deref(), Some("Maddy Makes Games"));
        assert_eq!(g.artwork.cover_url.as_deref(), Some("https://img/tall.jpg"));
        assert_eq!(g.artwork.banner_url.as_deref(), Some("https://img/wide.jpg"));
        assert_eq!(g.executable.as_ref().unwrap(), &game_dir.join("Celeste.exe"));
    }

    /// Cas réel (Windows) : dossier des paramètres sans manifeste, plugins Unreal sans
    /// catégorie dans le dossier du moteur.
    #[test]
    fn wrong_override_falls_back_and_engine_plugins_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let manifests = tmp.path().join("Manifests");
        let wrong = tmp.path().join("FortniteGame");
        let ue = tmp.path().join("UE_5.4");
        let fortnite = tmp.path().join("Fortnite");
        for d in [&manifests, &wrong, &ue, &fortnite] {
            fs::create_dir_all(d).unwrap();
        }
        let esc = |p: &Path| p.display().to_string().replace('\\', "\\\\");
        let item = |name: &str, app: &str, cats: &str, loc: &Path| {
            fs::write(
                manifests.join(format!("{app}.item")),
                format!(r#"{{"DisplayName":"{name}","AppName":"{app}","AppCategories":[{cats}],"InstallLocation":"{}"}}"#, esc(loc)),
            )
            .unwrap()
        };
        // Manifeste réel de Fortnite : `MainGameAppName` vide.
        fs::write(
            manifests.join("Fortnite.item"),
            format!(
                r#"{{"DisplayName":"Fortnite","AppName":"Fortnite","MainGameAppName":"","AppCategories":["games","applications"],"InstallLocation":"{}","bIsIncompleteInstall":false}}"#,
                esc(&fortnite)
            ),
        )
        .unwrap();
        item("Contenu de Fortnite", "aa31f9", r#""hidden""#, &fortnite);
        item("Unreal Engine", "UE_5.4", r#""engines/ue5","engines""#, &ue);
        item("Fab UE Plugin", "FabPlugin_5.4", "", &ue);
        item("Quixel Bridge", "QuixelBridge_5.4", "", &ue);
        item("Discord", "discord", r#""public","software","applications""#, &fortnite);

        let scanner = EpicScanner::new(Some(wrong.clone()));
        assert_eq!(scanner.ignored_override(), Some(wrong.as_path()));
        // Sans détection automatique possible ici, le scanner tombe sur le dossier fourni par le test.
        let report = EpicScanner::new(Some(manifests.clone())).scan(&ScanContext::silent()).unwrap();
        let names: Vec<_> = report.games.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, vec!["Fortnite"]);
        assert!(EpicScanner::new(Some(manifests)).ignored_override().is_none());
    }

    #[test]
    fn reads_heroic_and_legendary_installations() {
        let tmp = tempfile::tempdir().unwrap();
        let game_dir = tmp.path().join("Games/Hades");
        fs::create_dir_all(&game_dir).unwrap();
        let installed = tmp.path().join("installed.json");
        fs::write(
            &installed,
            serde_json::json!({
                "Min": {"app_name": "Min", "title": "Hades", "install_path": game_dir, "executable": "Hades.exe",
                        "install_size": 15_000_000_000u64, "is_dlc": false},
                "MinDLC": {"app_name": "MinDLC", "title": "Hades OST", "install_path": game_dir, "is_dlc": true}
            })
            .to_string(),
        )
        .unwrap();
        let lib = tmp.path().join("legendary_library.json");
        fs::write(
            &lib,
            r#"{"library":[{"app_name":"Min","art_square":"https://img/tall.jpg","art_cover":"https://img/wide.jpg","developer":"Supergiant"}]}"#,
        )
        .unwrap();

        let heroic = LegendaryConfig { installed: installed.clone(), heroic_library: Some(lib), heroic: true };
        let games = scan_legendary(&heroic).unwrap();
        assert_eq!(games.len(), 1);
        let g = &games[0];
        assert!(g.installed);
        assert_eq!(g.launch_uri.as_deref(), Some("heroic://launch?appName=Min&runner=legendary"));
        assert_eq!(g.artwork.cover_url.as_deref(), Some("https://img/tall.jpg"));
        assert_eq!(g.developer.as_deref(), Some("Supergiant"));

        let plain = LegendaryConfig { installed: installed.clone(), heroic_library: None, heroic: false };
        let g = &scan_legendary(&plain).unwrap()[0];
        assert_eq!(g.launch_uri.as_deref(), Some("legendary://Min"));
        assert!(g.update.is_none(), "sans cache de versions : état inconnu, rien d'inventé");

        // Versions : installée 1.0, publiée 1.1 → mise à jour disponible.
        let mut v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&installed).unwrap()).unwrap();
        v["Min"]["version"] = "1.0".into();
        fs::write(&installed, v.to_string()).unwrap();
        fs::write(
            tmp.path().join("assets.json"),
            r#"{"Windows": [{"app_name": "Min", "build_version": "1.1"}]}"#,
        )
        .unwrap();
        let g = &scan_legendary(&plain).unwrap()[0];
        assert_eq!(g.version.as_deref(), Some("1.0"));
        assert_eq!(g.update.unwrap().state, UpdateState::Available);
    }
}
