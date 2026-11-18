//! Autres launchers (1.1) : Ubisoft Connect, EA app, Xbox / Game Pass PC, Battle.net, Riot,
//! Amazon Games et itch.io. Jeux **installés**, lus sans compte ni configuration dans ce que
//! chaque launcher laisse sur le PC (registre, fichiers de configuration) ; le jeu est lancé
//! par son launcher, qui garde la main sur la connexion, les mises à jour et le DRM.
//!
//! | Launcher | Lecture | Lancement |
//! |---|---|---|
//! | Ubisoft Connect | `HKLM\…\Ubisoft\Launcher\Installs\<id>` + entrée de désinstallation (nom) | `uplay://launch/<id>/0` |
//! | EA app | `%ProgramData%\EA Desktop\InstallData\<Jeu>\base-<offre>` | `origin2://game/launch?offerIds=…` |
//! | Xbox / Game Pass | paquets du Microsoft Store contenant `MicrosoftGame.config` (PowerShell) | `explorer shell:AppsFolder\<AUMID>` |
//! | Battle.net | entrées de désinstallation « Blizzard Uninstaller » (`--uid=`) | `Battle.net.exe --exec="launch <code>"` |
//! | Riot | `%ProgramData%\Riot Games\Metadata\<produit>.live\*.product_settings.yaml` | `RiotClientServices.exe --launch-product=…` |
//! | Amazon Games | entrées de désinstallation « Amazon Game Remover » (`-p <id>`) | `amazon-games://play/<id>` |
//! | itch.io | `<config>/itch/apps/<jeu>/.itch/receipt.json.gz` | exécutable du jeu |
//!
//! Méthodes de lecture relevées dans CLRE-Launcher (licence MIT), réécrites ici.
//! Les fonctions d'analyse sont pures (testées sur toutes les plateformes) ; seules les
//! lectures du registre et de PowerShell sont propres à Windows.

use halcyon_core::{DiscoveredGame, GameScanner, GameSource, ScanContext, ScanReport};
use std::path::{Path, PathBuf};

/// Lecteur d'un launcher.
pub struct LauncherScanner {
    source: GameSource,
}

impl LauncherScanner {
    pub fn new(source: GameSource) -> Self {
        debug_assert!(GameSource::LAUNCHERS.contains(&source));
        Self { source }
    }

    /// Le launcher existe-t-il sur ce système d'exploitation ?
    pub fn supported(source: GameSource) -> bool {
        source == GameSource::Itch || cfg!(windows)
    }

    /// Emplacement affiché dans les paramètres (`None` : launcher non trouvé).
    pub fn location(source: GameSource) -> Option<String> {
        match source {
            GameSource::Itch => itch_apps_dirs().into_iter().next().map(|p| p.display().to_string()),
            GameSource::Ea => Some(ea_root()).filter(|p| p.is_dir()).map(|p| p.display().to_string()),
            GameSource::Riot => Some(riot_metadata()).filter(|p| p.is_dir()).map(|p| p.display().to_string()),
            _ if cfg!(windows) => {
                let found = match source {
                    GameSource::Ubisoft => win::ubisoft_installs().map(|v| !v.is_empty()),
                    GameSource::Xbox => Some(true),
                    GameSource::BattleNet => Some(win::uninstall_entries().iter().any(is_battlenet_client)),
                    GameSource::Amazon => Some(win::uninstall_entries().iter().any(|e| amazon_id(e).is_some())),
                    _ => None,
                };
                found.unwrap_or(false).then(|| source.label())
            }
            _ => None,
        }
    }
}

impl GameScanner for LauncherScanner {
    fn source(&self) -> GameSource {
        self.source
    }

    fn scan(&self, ctx: &ScanContext) -> halcyon_core::Result<ScanReport> {
        let label = self.source.label();
        if !Self::supported(self.source) {
            return Err(crate::io_err(tr!("{label} n'existe que sous Windows", label = label)));
        }
        ctx.progress(&tr!("{label} : lecture des jeux installés…", label = label));
        let games = match self.source {
            GameSource::Ubisoft => scan_ubisoft(),
            GameSource::Ea => scan_ea(&ea_root()),
            GameSource::Xbox => scan_xbox(),
            GameSource::BattleNet => Ok(battlenet_games(&win::uninstall_entries())),
            GameSource::Riot => scan_riot(&riot_metadata(), riot_client().as_deref()),
            GameSource::Amazon => Ok(amazon_games(&win::uninstall_entries())),
            GameSource::Itch => scan_itch(&itch_apps_dirs()),
            _ => Ok(Vec::new()),
        }
        .map_err(crate::io_err)?;
        log::info!("{label} : {} jeux trouvés", games.len());
        Ok(ScanReport { games, complete: true, ..Default::default() })
    }
}

fn program_data() -> PathBuf {
    std::env::var_os("ProgramData").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
}

// ------------------------------------------------------------------ entrées de désinstallation

/// Programme installé, tel que Windows le déclare (clés `Uninstall`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UninstallEntry {
    pub key: String,
    pub name: String,
    pub install_location: String,
    pub uninstall_string: String,
}

/// Valeur d'une option `--nom=valeur` ou `-n valeur` dans une ligne de commande.
fn option_value(cmd: &str, flag: &str) -> Option<String> {
    // Option précédée d'une espace : « -p » ne doit pas être trouvé au milieu d'un chemin.
    let lower = cmd.to_ascii_lowercase();
    let at = lower.find(&format!(" {}", flag.to_ascii_lowercase()))? + flag.len() + 1;
    let rest = cmd[at..].trim_start_matches(['=', ' ']).trim_start_matches('"');
    let end = rest.find(['"', ' ']).unwrap_or(rest.len());
    Some(rest[..end].to_string()).filter(|v| !v.is_empty())
}

// ------------------------------------------------------------------ Ubisoft Connect

fn scan_ubisoft() -> Result<Vec<DiscoveredGame>, String> {
    let installs = win::ubisoft_installs().ok_or_else(|| tr!("Ubisoft Connect introuvable"))?;
    let names: Vec<UninstallEntry> = win::uninstall_entries();
    Ok(ubisoft_games(&installs, &names))
}

/// `(identifiant, dossier)` des jeux Ubisoft ; le nom vient de l'entrée « Uplay Install <id> »
/// quand elle existe, sinon du nom du dossier.
pub fn ubisoft_games(installs: &[(String, String)], entries: &[UninstallEntry]) -> Vec<DiscoveredGame> {
    installs
        .iter()
        .filter(|(id, dir)| id.chars().all(|c| c.is_ascii_digit()) && !dir.trim().is_empty())
        .map(|(id, dir)| {
            let dir = dir.trim().trim_end_matches(['\\', '/']);
            let path = PathBuf::from(dir);
            let folder = dir.rsplit(['\\', '/']).next().unwrap_or(dir).to_string();
            let from_entry = entries.iter().find(|e| e.key.rsplit('\\').next() == Some(&format!("Uplay Install {id}"))).map(|e| e.name.trim().to_string());
            let name = from_entry
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| if folder.is_empty() { id.clone() } else { folder });
            let mut g = DiscoveredGame::new(id.clone(), name);
            g.installed = path.is_dir();
            g.install_dir = Some(path);
            g.launch_uri = Some(format!("uplay://launch/{id}/0"));
            g
        })
        .collect()
}

// ------------------------------------------------------------------ EA app

fn ea_root() -> PathBuf {
    program_data().join("EA Desktop").join("InstallData")
}

/// EA app : un dossier par jeu, contenant `base-<offre>`. L'EA app ne dit pas où le jeu est
/// installé : c'est elle qui le lance (pas de suivi du temps de jeu par dossier).
pub fn scan_ea(root: &Path) -> Result<Vec<DiscoveredGame>, String> {
    let rd = std::fs::read_dir(root).map_err(|_| tr!("EA app introuvable"))?;
    let mut out = Vec::new();
    for e in rd.flatten().filter(|e| e.path().is_dir()) {
        let Ok(files) = std::fs::read_dir(e.path()) else { continue };
        let Some(offer) = files.flatten().find_map(|f| f.file_name().to_str()?.strip_prefix("base-").map(str::to_string)) else { continue };
        let folder = e.file_name().to_string_lossy().into_owned();
        let mut g = DiscoveredGame::new(offer.clone(), split_camel(&folder));
        g.launch_uri = Some(format!("origin2://game/launch?offerIds={}", crate::plugins::encode(&offer)));
        out.push(g);
    }
    Ok(out)
}

/// « ItTakesTwo » → « It Takes Two » (un nom déjà espacé est laissé tel quel).
pub fn split_camel(s: &str) -> String {
    let mut out = String::new();
    let mut prev: Option<char> = None;
    for c in s.chars() {
        if c.is_uppercase() && prev.is_some_and(|p| p.is_lowercase() || p.is_ascii_digit()) {
            out.push(' ');
        }
        out.push(c);
        prev = Some(c);
    }
    out.replace('_', " ").trim().to_string()
}

// ------------------------------------------------------------------ Xbox / Game Pass

/// Paquets du Microsoft Store qui sont des jeux (`MicrosoftGame.config`), en JSON.
/// Script constant : aucune valeur extérieure n'y est insérée.
const PS_XBOX: &str = r#"[Console]::OutputEncoding = [Text.Encoding]::UTF8
$ErrorActionPreference = 'SilentlyContinue'
$out = foreach ($p in Get-AppxPackage) {
  if ($p.IsFramework -or $p.SignatureKind -eq 'System') { continue }
  $cfg = Join-Path $p.InstallLocation 'MicrosoftGame.config'
  if (-not (Test-Path -LiteralPath $cfg)) { continue }
  [xml]$x = Get-Content -Raw -LiteralPath $cfg
  $app = (Get-AppxPackageManifest $p).Package.Applications.Application | Select-Object -First 1
  [pscustomobject]@{
    pfn = $p.PackageFamilyName; appId = $app.Id; dir = $p.InstallLocation
    name = $x.Game.ShellVisuals.DefaultDisplayName; publisher = $x.Game.ShellVisuals.PublisherDisplayName
    exe = ($x.Game.ExecutableList.Executable | Select-Object -First 1).Name
  }
}
ConvertTo-Json -Compress @($out)"#;

fn scan_xbox() -> Result<Vec<DiscoveredGame>, String> {
    let json = win::powershell(PS_XBOX).ok_or_else(|| tr!("liste des applications du Microsoft Store indisponible"))?;
    Ok(xbox_games(&json, &win::explorer()))
}

/// Jeux Xbox d'après la sortie JSON du script ; lancés par l'Explorateur
/// (`shell:AppsFolder\<famille>!<application>`), comme le menu Démarrer.
pub fn xbox_games(json: &str, explorer: &Path) -> Vec<DiscoveredGame> {
    let v: serde_json::Value = serde_json::from_str(json.trim()).unwrap_or(serde_json::Value::Null);
    let list = match v {
        serde_json::Value::Array(a) => a,
        serde_json::Value::Object(_) => vec![v],
        _ => Vec::new(),
    };
    list.iter()
        .filter_map(|x| {
            let s = |k: &str| x.get(k).and_then(|v| v.as_str()).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string);
            let (pfn, app, name) = (s("pfn")?, s("appId")?, s("name")?);
            // Nom non résolu (« ms-resource:… ») : pas de nom lisible, jeu ignoré.
            if name.to_ascii_lowercase().starts_with("ms-resource:") || !pfn.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) {
                return None;
            }
            let mut g = DiscoveredGame::new(pfn.clone(), name);
            g.publisher = s("publisher");
            g.install_dir = s("dir").map(PathBuf::from);
            g.executable = Some(explorer.to_path_buf());
            g.arguments = format!("shell:AppsFolder\\{pfn}!{app}");
            Some(g)
        })
        .collect()
}

// ------------------------------------------------------------------ Battle.net

/// Code de lancement Battle.net (`--exec="launch <code>"`) par identifiant de désinstallation.
const BNET_CODES: &[(&str, &str)] = &[
    ("wow", "WoW"),
    ("wow_classic", "WoWC"),
    ("wow_classic_era", "WoWC"),
    ("pro", "Pro"),
    ("prometheus", "Pro"),
    ("fenris", "Fen"),
    ("osi", "OSI"),
    ("hs_beta", "WTCG"),
    ("s2", "S2"),
    ("s1", "S1"),
    ("heroes", "Hero"),
    ("d3", "D3"),
    ("odin", "ODIN"),
    ("w3", "W3"),
    ("wlby", "WLBY"),
    ("zeus", "ZEUS"),
    ("viper", "VIPR"),
    ("lazarus", "LAZR"),
    ("fore", "FORE"),
    ("auks", "AUKS"),
    ("rtro", "RTRO"),
    ("anbs", "ANBS"),
    ("gryphon", "GRY"),
    ("spot", "SPOT"),
    ("aqua", "AQUA"),
];

fn is_battlenet_client(e: &UninstallEntry) -> bool {
    option_value(&e.uninstall_string, "--uid").is_some_and(|u| u.eq_ignore_ascii_case("battle.net"))
}

pub fn battlenet_games(entries: &[UninstallEntry]) -> Vec<DiscoveredGame> {
    let client = entries
        .iter()
        .find(|e| is_battlenet_client(e))
        .map(|e| PathBuf::from(e.install_location.trim()).join("Battle.net.exe"))
        .filter(|p| p.is_file());
    entries
        .iter()
        .filter(|e| e.uninstall_string.to_ascii_lowercase().contains("blizzard uninstaller") && !is_battlenet_client(e))
        .filter_map(|e| {
            let uid = option_value(&e.uninstall_string, "--uid")?.to_ascii_lowercase();
            let code = BNET_CODES
                .iter()
                .find(|(k, _)| *k == uid)
                .or_else(|| BNET_CODES.iter().find(|(k, _)| uid.split('_').next() == Some(k)))
                .map(|(_, c)| *c);
            let dir = PathBuf::from(e.install_location.trim());
            let mut g = DiscoveredGame::new(uid.clone(), e.name.trim().to_string());
            g.installed = dir.is_dir();
            g.install_dir = (!e.install_location.trim().is_empty()).then_some(dir);
            match (&client, code) {
                (Some(exe), Some(code)) => {
                    g.executable = Some(exe.clone());
                    g.arguments = format!("\"--exec=launch {code}\"");
                }
                // Client ou code inconnus : Battle.net s'ouvre sur le jeu.
                _ => g.launch_uri = Some(format!("battlenet://{}", code.unwrap_or(""))),
            }
            Some(g)
        })
        .collect()
}

// ------------------------------------------------------------------ Riot

fn riot_metadata() -> PathBuf {
    program_data().join("Riot Games").join("Metadata")
}

/// Riot Client : `RiotClientInstalls.json`, sinon l'emplacement habituel.
fn riot_client() -> Option<PathBuf> {
    let file = program_data().join("Riot Games").join("RiotClientInstalls.json");
    let from_json = crate::read_text(&file).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).and_then(|v| {
        ["rc_default", "rc_live"].iter().filter_map(|k| v.get(*k)?.as_str().map(PathBuf::from)).find(|p| p.is_file())
    });
    from_json.or_else(|| Some(PathBuf::from(r"C:\Riot Games\Riot Client\RiotClientServices.exe")).filter(|p| p.is_file()))
}

/// Produits Riot connus : identifiant, nom.
const RIOT: &[(&str, &str)] = &[
    ("league_of_legends", "League of Legends"),
    ("valorant", "VALORANT"),
    ("bacon", "Legends of Runeterra"),
    ("lion", "2XKO"),
];

pub fn scan_riot(metadata: &Path, client: Option<&Path>) -> Result<Vec<DiscoveredGame>, String> {
    let rd = std::fs::read_dir(metadata).map_err(|_| tr!("Riot Client introuvable"))?;
    let mut out = Vec::new();
    for e in rd.flatten() {
        let folder = e.file_name().to_string_lossy().into_owned();
        let Some(product) = folder.strip_suffix(".live") else { continue };
        let Some((_, name)) = RIOT.iter().find(|(id, _)| *id == product) else { continue };
        let Ok(yaml) = crate::read_text(&e.path().join(format!("{folder}.product_settings.yaml"))) else { continue };
        let dir = riot_install_path(&yaml).map(PathBuf::from);
        let mut g = DiscoveredGame::new(product.to_string(), *name);
        // Le dossier peut rester après une désinstallation : le client doit aussi être là.
        g.installed = dir.as_ref().is_some_and(|d| d.is_dir()) && client.is_some();
        g.install_dir = dir;
        if let Some(c) = client {
            g.executable = Some(c.to_path_buf());
            g.arguments = format!("--launch-product={product} --launch-patchline=live");
        }
        out.push(g);
    }
    Ok(out)
}

/// `product_install_full_path: "C:/Riot Games/VALORANT/live"` dans le YAML du produit.
pub fn riot_install_path(yaml: &str) -> Option<String> {
    yaml.lines().find_map(|l| {
        let v = l.trim().strip_prefix("product_install_full_path:")?.trim().trim_matches(['"', '\'']);
        (!v.is_empty()).then(|| if cfg!(windows) { v.replace('/', "\\") } else { v.to_string() })
    })
}

// ------------------------------------------------------------------ Amazon Games

fn amazon_id(e: &UninstallEntry) -> Option<String> {
    e.uninstall_string.to_ascii_lowercase().contains("amazon game remover").then(|| option_value(&e.uninstall_string, "-p")).flatten()
}

pub fn amazon_games(entries: &[UninstallEntry]) -> Vec<DiscoveredGame> {
    entries
        .iter()
        .filter_map(|e| {
            let id = amazon_id(e)?;
            if !id.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) {
                return None;
            }
            let dir = PathBuf::from(e.install_location.trim());
            let mut g = DiscoveredGame::new(id.clone(), e.name.trim().to_string());
            g.installed = dir.is_dir();
            g.install_dir = (!e.install_location.trim().is_empty()).then_some(dir);
            g.launch_uri = Some(format!("amazon-games://play/{id}"));
            Some(g)
        })
        .collect()
}

// ------------------------------------------------------------------ itch.io

/// Dossiers `apps` de l'application itch (Windows : `%APPDATA%\itch`, Linux : `~/.config/itch`).
fn itch_apps_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(cfg) = dirs::config_dir() {
        v.push(cfg.join("itch").join("apps"));
    }
    if let Some(home) = dirs::home_dir() {
        v.push(home.join(".var/app/io.itch.itch/config/itch/apps"));
    }
    v.into_iter().filter(|p| p.is_dir()).collect()
}

pub fn scan_itch(roots: &[PathBuf]) -> Result<Vec<DiscoveredGame>, String> {
    if roots.is_empty() {
        return Err(tr!("application itch introuvable"));
    }
    let mut out = Vec::new();
    for root in roots {
        let Ok(rd) = std::fs::read_dir(root) else { continue };
        for e in rd.flatten().filter(|e| e.path().is_dir()) {
            let dir = e.path();
            let Some(receipt) = read_receipt(&dir.join(".itch").join("receipt.json.gz")) else { continue };
            if let Some(g) = itch_game(&receipt, &dir) {
                out.push(g);
            }
        }
    }
    Ok(out)
}

fn read_receipt(path: &Path) -> Option<serde_json::Value> {
    use std::io::Read;
    let f = std::fs::File::open(path).ok()?;
    let mut text = String::new();
    // Reçu de quelques Ko : lecture plafonnée.
    flate2::read::GzDecoder::new(f).take(4 * 1024 * 1024).read_to_string(&mut text).ok()?;
    serde_json::from_str(&text).ok()
}

/// Jeu itch d'après son reçu ; exécutable cherché dans le dossier (2 niveaux).
pub fn itch_game(receipt: &serde_json::Value, dir: &Path) -> Option<DiscoveredGame> {
    let game = receipt.get("game")?;
    let id = game.get("id").and_then(|v| v.as_u64().map(|n| n.to_string()).or_else(|| v.as_str().map(str::to_string)))?;
    let title = game.get("title").and_then(|v| v.as_str()).map(str::trim).filter(|t| !t.is_empty())?.to_string();
    let mut g = DiscoveredGame::new(id, title);
    g.install_dir = Some(dir.to_path_buf());
    g.executable = find_executable(dir, 0);
    g.working_dir = g.executable.as_ref().and_then(|e| e.parent().map(Path::to_path_buf));
    let url = |k: &str| game.get(k).and_then(|v| v.as_str()).filter(|u| u.starts_with("https://")).map(str::to_string);
    g.artwork.cover_url = url("stillCoverUrl").or_else(|| url("coverUrl"));
    g.description = game.get("shortText").and_then(|v| v.as_str()).map(str::trim).filter(|t| !t.is_empty()).map(str::to_string);
    Some(g)
}

/// Premier exécutable plausible (pas un désinstalleur ni un programme d'installation).
fn find_executable(dir: &Path, depth: usize) -> Option<PathBuf> {
    const SKIP: &[&str] = &["unins", "crash", "setup", "redist", "vc_", "vcredist", "dxsetup", "ue4prereq", "dotnet", "install"];
    let mut entries: Vec<_> = std::fs::read_dir(dir).ok()?.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    let plausible = |p: &Path| {
        let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        let runnable = if cfg!(windows) {
            name.ends_with(".exe")
        } else {
            name.ends_with(".x86_64") || name.ends_with(".sh") || name.ends_with(".appimage") || is_unix_exec(p)
        };
        runnable && !SKIP.iter().any(|s| name.contains(s))
    };
    if let Some(f) = entries.iter().map(|e| e.path()).find(|p| p.is_file() && plausible(p)) {
        return Some(f);
    }
    if depth < 2 {
        for e in entries.iter().filter(|e| e.path().is_dir() && e.file_name() != ".itch") {
            if let Some(f) = find_executable(&e.path(), depth + 1) {
                return Some(f);
            }
        }
    }
    None
}

#[cfg(unix)]
fn is_unix_exec(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    // Binaire sans extension avec le droit d'exécution (pas une bibliothèque).
    p.extension().is_none() && p.metadata().is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_unix_exec(_: &Path) -> bool {
    false
}

// ------------------------------------------------------------------ Windows

#[cfg(windows)]
mod win {
    use super::UninstallEntry;
    use std::path::PathBuf;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY};
    use winreg::RegKey;

    /// Programmes installés : clés `Uninstall` 64 et 32 bits de la machine, et de l'utilisateur.
    pub fn uninstall_entries() -> Vec<UninstallEntry> {
        const PATH: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
        let mut out = Vec::new();
        let roots = [
            (HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY),
            (HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY),
            (HKEY_CURRENT_USER, KEY_WOW64_64KEY),
        ];
        for (hive, view) in roots {
            let Ok(root) = RegKey::predef(hive).open_subkey_with_flags(PATH, KEY_READ | view) else { continue };
            for name in root.enum_keys().flatten() {
                let Ok(k) = root.open_subkey_with_flags(&name, KEY_READ | view) else { continue };
                let get = |v: &str| k.get_value::<String, _>(v).unwrap_or_default();
                let e = UninstallEntry { key: format!("{PATH}\\{name}"), name: get("DisplayName"), install_location: get("InstallLocation"), uninstall_string: get("UninstallString") };
                if !e.name.is_empty() && !out.contains(&e) {
                    out.push(e);
                }
            }
        }
        out
    }

    /// `(identifiant, dossier)` des jeux déclarés par Ubisoft Connect.
    pub fn ubisoft_installs() -> Option<Vec<(String, String)>> {
        let root = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(r"SOFTWARE\Ubisoft\Launcher\Installs", KEY_READ | KEY_WOW64_32KEY).ok()?;
        Some(
            root.enum_keys()
                .flatten()
                .filter_map(|id| Some((id.clone(), root.open_subkey(&id).ok()?.get_value::<String, _>("InstallDir").ok()?)))
                .collect(),
        )
    }

    pub fn powershell(script: &str) -> Option<String> {
        use std::os::windows::process::CommandExt;
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script])
            .creation_flags(0x0800_0000)
            .output()
            .ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    }

    pub fn explorer() -> PathBuf {
        std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows")).join("explorer.exe")
    }
}

#[cfg(not(windows))]
mod win {
    use super::UninstallEntry;
    use std::path::PathBuf;

    pub fn uninstall_entries() -> Vec<UninstallEntry> {
        Vec::new()
    }
    pub fn ubisoft_installs() -> Option<Vec<(String, String)>> {
        None
    }
    pub fn powershell(_: &str) -> Option<String> {
        None
    }
    pub fn explorer() -> PathBuf {
        PathBuf::from("explorer.exe")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: &str, name: &str, dir: &str, uninstall: &str) -> UninstallEntry {
        UninstallEntry { key: key.into(), name: name.into(), install_location: dir.into(), uninstall_string: uninstall.into() }
    }

    #[test]
    fn ubisoft_names_from_uninstall_entries() {
        let installs = vec![("635".to_string(), r"C:\Games\Assassin's Creed Origins\".to_string()), ("x".into(), "C:\\nope".into()), ("720".into(), r"D:\Ubi\Far Cry 6".into())];
        let entries = vec![entry(r"SOFTWARE\...\Uninstall\Uplay Install 635", "Assassin's Creed® Origins", "", "")];
        let g = ubisoft_games(&installs, &entries);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].name, "Assassin's Creed® Origins");
        assert_eq!(g[0].launch_uri.as_deref(), Some("uplay://launch/635/0"));
        assert_eq!(g[1].name, "Far Cry 6");
    }

    #[test]
    fn ea_offers_and_names() {
        let t = tempfile::tempdir().unwrap();
        for (dir, file) in [("ItTakesTwo", "base-Origin.OFR.50.0004104"), ("Battlefield 2042", "base-Origin.OFR.50.0004390"), ("Vide", "readme.txt")] {
            std::fs::create_dir_all(t.path().join(dir)).unwrap();
            std::fs::write(t.path().join(dir).join(file), "").unwrap();
        }
        let mut g = scan_ea(t.path()).unwrap();
        g.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(g.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["Battlefield 2042", "It Takes Two"]);
        assert_eq!(g[1].launch_uri.as_deref(), Some("origin2://game/launch?offerIds=Origin.OFR.50.0004104"));
        assert!(scan_ea(&t.path().join("absent")).is_err());
        assert_eq!(split_camel("FIFA23"), "FIFA23");
    }

    #[test]
    fn xbox_packages_from_powershell_json() {
        let json = r#"[{"pfn":"Microsoft.624F8B84B80_8wekyb3d8bbwe","appId":"Forza","dir":"C:\\XboxGames\\Forza Horizon 5\\Content","name":"Forza Horizon 5","publisher":"Xbox Game Studios","exe":"ForzaHorizon5.exe"},
                       {"pfn":"Bad.Pkg_x","appId":"App","name":"ms-resource:Title"},
                       {"pfn":"evil & calc","appId":"A","name":"Evil"}]"#;
        let g = xbox_games(json, Path::new(r"C:\Windows\explorer.exe"));
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].arguments, r"shell:AppsFolder\Microsoft.624F8B84B80_8wekyb3d8bbwe!Forza");
        assert_eq!(g[0].publisher.as_deref(), Some("Xbox Game Studios"));
        // Un seul paquet : PowerShell renvoie un objet, pas une liste.
        assert_eq!(xbox_games(r#"{"pfn":"A.B_c","appId":"G","name":"Jeu"}"#, Path::new("explorer.exe")).len(), 1);
        assert!(xbox_games("", Path::new("explorer.exe")).is_empty());
    }

    #[test]
    fn battlenet_games_and_launch_codes() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("Battle.net.exe"), "").unwrap();
        let client_dir = t.path().display().to_string();
        let entries = vec![
            entry("k1", "Battle.net", &client_dir, r#""C:\ProgramData\Battle.net\Agent\Blizzard Uninstaller.exe" --lang=frFR --uid=battle.net --displayname="Battle.net""#),
            entry("k2", "Jeu futur", &client_dir, r#""C:\ProgramData\Battle.net\Agent\Blizzard Uninstaller.exe" --lang=frFR --uid=nouveau --displayname="Jeu futur""#),
            entry("k3", "Diablo IV", &client_dir, r#""C:\ProgramData\Battle.net\Agent\Blizzard Uninstaller.exe" --uid=fenris --displayname="Diablo IV""#),
            entry("k4", "Autre", "", "msiexec /x {123}"),
        ];
        let g = battlenet_games(&entries);
        assert_eq!(g.len(), 2);
        // Identifiant sans code connu : Battle.net s'ouvre.
        assert_eq!(g[0].launch_uri.as_deref(), Some("battlenet://"));
        assert_eq!(g[1].arguments, "\"--exec=launch Fen\"");
        assert!(g[1].executable.as_ref().unwrap().ends_with("Battle.net.exe"));
    }

    #[test]
    fn riot_products() {
        let t = tempfile::tempdir().unwrap();
        let meta = t.path().join("Metadata");
        let game = t.path().join("VALORANT/live");
        std::fs::create_dir_all(&game).unwrap();
        for (folder, body) in [
            ("valorant.live", format!("product_install_full_path: \"{}\"\nproduct_install_root: \"x\"\n", game.display())),
            ("inconnu.live", "product_install_full_path: \"C:/x\"".into()),
            ("league_of_legends.live", "autre: 1".into()),
        ] {
            std::fs::create_dir_all(meta.join(folder)).unwrap();
            std::fs::write(meta.join(folder).join(format!("{folder}.product_settings.yaml")), body).unwrap();
        }
        let client = t.path().join("RiotClientServices.exe");
        let mut g = scan_riot(&meta, Some(&client)).unwrap();
        g.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(g.len(), 2);
        assert_eq!(g[1].name, "VALORANT");
        assert!(g[1].installed);
        assert_eq!(g[1].arguments, "--launch-product=valorant --launch-patchline=live");
        assert!(!g[0].installed, "League sans dossier d'installation");
        assert!(!scan_riot(&meta, None).unwrap().iter().any(|g| g.installed), "client absent");
    }

    #[test]
    fn amazon_games_from_uninstall_entries() {
        let entries = vec![
            entry("a", "Fallout 76", r"C:\Amazon Games\Library\Fallout 76", r#""C:\Users\x\AppData\Local\Amazon Games\App\Amazon Game Remover.exe" -m Game -p amzn1.adg.product.1234-abcd"#),
            entry("b", "Autre", "", "uninstall.exe"),
            entry("c", "Piège", "", r#"Amazon Game Remover.exe -p "a&b""#),
        ];
        let g = amazon_games(&entries);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].launch_uri.as_deref(), Some("amazon-games://play/amzn1.adg.product.1234-abcd"));
    }

    #[test]
    fn itch_receipts() {
        use std::io::Write;
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("celeste");
        std::fs::create_dir_all(dir.join(".itch")).unwrap();
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        let exe = if cfg!(windows) { "Celeste.exe" } else { "Celeste.x86_64" };
        std::fs::write(dir.join("bin").join(exe), "").unwrap();
        std::fs::write(dir.join(if cfg!(windows) { "unins000.exe" } else { "uninstall.sh" }), "").unwrap();
        let receipt = serde_json::json!({"game": {"id": 123, "title": "Celeste", "shortText": "Grimpe.", "coverUrl": "https://img.itch.zone/c.png"}});
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(receipt.to_string().as_bytes()).unwrap();
        std::fs::write(dir.join(".itch/receipt.json.gz"), gz.finish().unwrap()).unwrap();
        let g = scan_itch(&[t.path().to_path_buf()]).unwrap();
        assert_eq!(g.len(), 1);
        assert_eq!((g[0].source_key.as_str(), g[0].name.as_str()), ("123", "Celeste"));
        assert!(g[0].executable.as_ref().unwrap().ends_with(exe));
        assert_eq!(g[0].artwork.cover_url.as_deref(), Some("https://img.itch.zone/c.png"));
        assert!(scan_itch(&[]).is_err());
    }

    #[test]
    fn command_line_options() {
        assert_eq!(option_value(r#"C:\x-p\a.exe --uid=prometheus"#, "-p"), None);
        assert_eq!(option_value(r#"x.exe --uid=fenris --x"#, "--uid").as_deref(), Some("fenris"));
        assert_eq!(option_value(r#"x.exe -m Game -p amzn1.x"#, "-p").as_deref(), Some("amzn1.x"));
        assert_eq!(option_value("x.exe", "--uid"), None);
    }
}
