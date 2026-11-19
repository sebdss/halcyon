//! Désinstallation (1.0.0-alpha.6) : quelle voie pour quel jeu, et garde-fous de chemin.
//!
//! Halcyon ne remplace jamais les plateformes : un jeu Steam se désinstalle par Steam, un jeu
//! Legendary par Legendary, les autres launchers ouvrent leur propre page. Seul le dossier
//! d'un jeu **autonome** (détecté dans un dossier de jeux enregistré) peut être supprimé par
//! Halcyon, et seulement s'il passe [`check_deletable`]. Les ROM et les fichiers ajoutés à la
//! main ne sont jamais supprimés : ils sont seulement retirés de la bibliothèque.
//!
//! Logique pure : aucune entrée/sortie ici (la corbeille et les processus sont dans l'app).

use crate::model::{Game, GameSource, LEGENDARY_URI};
use std::path::{Component, Path, PathBuf};

/// Voie de désinstallation d'un jeu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UninstallPlan {
    /// URI ouverte par le gestionnaire de protocole (Steam : `steam://uninstall/<appid>`).
    Uri(String),
    /// Commande du launcher, lancée directement (aucun shell) ; `stdin` répond à sa
    /// confirmation, déjà donnée dans Halcyon.
    Command { program: String, args: Vec<String>, stdin: Option<String> },
    /// La désinstallation se termine dans le launcher : Halcyon ouvre sa page (`uri`), ou
    /// rien s'il n'y en a pas, et indique où aller.
    Delegate { uri: Option<String>, launcher: String },
    /// Dossier d'un jeu autonome, envoyé à la corbeille (après [`check_deletable`]).
    DeleteFolder(PathBuf),
    /// Jeu ajouté à la main : retiré de la bibliothèque, aucun fichier touché.
    RemoveFromLibrary,
    /// ROM : masquée de la bibliothèque (une nouvelle analyse ne la fait pas revenir), aucun
    /// fichier touché.
    HideFromLibrary,
    /// Rien à faire : déjà non installé, ou géré ailleurs (onglet Minecraft).
    Unavailable(UnavailableReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnavailableReason {
    NotInstalled,
    Minecraft,
    NoInstallFolder,
}

/// Plan de désinstallation, d'après la source et la façon dont le jeu se lance.
/// `windows` : système Windows (les launchers sans URI y passent par « Applications »).
pub fn plan(g: &Game, windows: bool) -> UninstallPlan {
    match g.source {
        GameSource::Manual => return UninstallPlan::RemoveFromLibrary,
        GameSource::Emulated => return UninstallPlan::HideFromLibrary,
        GameSource::Minecraft => return UninstallPlan::Unavailable(UnavailableReason::Minecraft),
        _ => {}
    }
    if !g.installed {
        return UninstallPlan::Unavailable(UnavailableReason::NotInstalled);
    }
    let uri = g.launch_uri.as_deref().unwrap_or("");
    let apps_settings = || if windows { Some("ms-settings:appsfeatures".to_string()) } else { None };
    match g.source {
        GameSource::Steam if !g.source_key.is_empty() && g.source_key.chars().all(|c| c.is_ascii_digit()) => {
            UninstallPlan::Uri(format!("steam://uninstall/{}", g.source_key))
        }
        GameSource::Epic | GameSource::Gog if uri.starts_with(LEGENDARY_URI) => {
            let app = &uri[LEGENDARY_URI.len()..];
            if valid_app_name(app) {
                UninstallPlan::Command { program: "legendary".into(), args: vec!["uninstall".into(), app.to_string()], stdin: Some("y\n".into()) }
            } else {
                UninstallPlan::Delegate { uri: None, launcher: "Legendary".into() }
            }
        }
        // Heroic n'a pas de commande de désinstallation : sa bibliothèque s'ouvre.
        _ if uri.starts_with("heroic://") => UninstallPlan::Delegate { uri: Some("heroic://".into()), launcher: "Heroic".into() },
        GameSource::Epic => UninstallPlan::Delegate { uri: Some("com.epicgames.launcher://store/library".into()), launcher: "Epic Games Launcher".into() },
        GameSource::Gog => UninstallPlan::Delegate {
            uri: (!g.source_key.is_empty() && g.source_key.chars().all(|c| c.is_ascii_digit()))
                .then(|| format!("goggalaxy://openGameView/{}", g.source_key))
                .or_else(apps_settings),
            launcher: "GOG Galaxy".into(),
        },
        GameSource::BattleNet if uri.starts_with("battlenet://") && uri.len() > "battlenet://".len() => {
            UninstallPlan::Delegate { uri: Some(uri.to_string()), launcher: "Battle.net".into() }
        }
        GameSource::Steam => UninstallPlan::Delegate { uri: Some("steam://open/games".into()), launcher: "Steam".into() },
        GameSource::Standalone => match &g.install_dir {
            Some(d) => UninstallPlan::DeleteFolder(d.clone()),
            None => UninstallPlan::Unavailable(UnavailableReason::NoInstallFolder),
        },
        s => UninstallPlan::Delegate { uri: apps_settings(), launcher: s.label().to_string() },
    }
}

/// Nom d'application Legendary : lettres, chiffres, tirets et soulignés seulement (il est
/// passé en argument, jamais à un shell, mais un nom étrange trahit une donnée corrompue).
fn valid_app_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) && !s.starts_with('-')
}

/// Raison d'un refus de suppression de dossier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Chemin relatif, ou contenant `..` / `.`.
    NotAbsolute,
    /// Racine d'un disque ou dossier de premier niveau (`/home`, `D:\Jeux`).
    TooShallow,
    /// Pas un sous-dossier direct d'un dossier de jeux enregistré.
    NotRegistered,
    /// Dossier protégé (dossier personnel, système, données de Halcyon…) ou l'un de ses parents.
    Protected(PathBuf),
    /// Contient (ou est) le dossier d'un autre jeu, une bibliothèque de ROM ou des sauvegardes.
    Shared(PathBuf),
}

impl Refusal {
    pub fn describe(&self) -> String {
        match self {
            Self::NotAbsolute => tr!("chemin incomplet"),
            Self::TooShallow => tr!("dossier trop proche de la racine du disque"),
            Self::NotRegistered => tr!("ce dossier n'est pas un dossier de jeu détecté dans vos dossiers de jeux"),
            Self::Protected(p) => tr!("dossier protégé : {path}", path = p.display()),
            Self::Shared(p) => tr!("le dossier contient aussi : {path}", path = p.display()),
        }
    }
}

fn normal_components(p: &Path) -> Option<Vec<String>> {
    let mut out = Vec::new();
    for c in p.components() {
        match c {
            Component::Prefix(px) => out.push(px.as_os_str().to_string_lossy().to_lowercase()),
            Component::RootDir => {}
            Component::Normal(s) => out.push(s.to_string_lossy().into_owned()),
            Component::CurDir | Component::ParentDir => return None,
        }
    }
    Some(out)
}

/// Comparaison de chemins : sensible à la casse sous Unix, insensible sous Windows.
fn same(a: &str, b: &str) -> bool {
    if cfg!(windows) {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

/// `a` est-il `b` ou l'un de ses parents ?
fn is_ancestor_or_same(a: &[String], b: &[String]) -> bool {
    a.len() <= b.len() && a.iter().zip(b).all(|(x, y)| same(x, y))
}

/// Le dossier d'installation `dir` peut-il être supprimé ? Garde-fous (mêmes principes que
/// les emplacements de sauvegarde Ludusavi : jamais une racine ni un dossier partagé) :
///
/// - chemin absolu, sans `..` ni `.` ;
/// - au moins deux niveaux sous la racine du disque ;
/// - **sous-dossier direct** d'un dossier de jeux enregistré (`registered`) : c'est là que
///   l'analyse a trouvé le jeu ; le dossier enregistré lui-même n'est jamais supprimé ;
/// - ni un dossier protégé (`protected` : dossier personnel, système, données de Halcyon,
///   Steam…) ni l'un de leurs parents ;
/// - ne contient ni le dossier d'un autre jeu, ni une bibliothèque de ROM, ni un dépôt de
///   sauvegardes (`others`), et n'est pas contenu dans l'un d'eux.
pub fn check_deletable(dir: &Path, registered: &[PathBuf], protected: &[PathBuf], others: &[PathBuf]) -> Result<(), Refusal> {
    if !dir.is_absolute() {
        return Err(Refusal::NotAbsolute);
    }
    let d = normal_components(dir).ok_or(Refusal::NotAbsolute)?;
    let depth = d.iter().filter(|c| !c.ends_with(':')).count();
    if depth < 2 {
        return Err(Refusal::TooShallow);
    }
    for p in protected {
        if let Some(pc) = normal_components(p) {
            // Supprimer un dossier protégé ou l'un de ses parents : refusé. Un jeu installé
            // *dans* un dossier protégé (ex. ~/Jeux/Celeste) reste possible.
            if is_ancestor_or_same(&d, &pc) {
                return Err(Refusal::Protected(p.clone()));
            }
        }
    }
    let registered_parent = registered.iter().filter_map(|r| normal_components(r)).any(|r| r.len() + 1 == d.len() && is_ancestor_or_same(&r, &d));
    if !registered_parent {
        return Err(Refusal::NotRegistered);
    }
    for o in others {
        if let Some(oc) = normal_components(o) {
            if is_ancestor_or_same(&d, &oc) || is_ancestor_or_same(&oc, &d) {
                return Err(Refusal::Shared(o.clone()));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Game;

    fn game(source: GameSource, key: &str, uri: Option<&str>, dir: Option<&str>) -> Game {
        let mut g = Game::blank(1, "Jeu", source);
        g.source_key = key.into();
        g.launch_uri = uri.map(str::to_string);
        g.install_dir = dir.map(PathBuf::from);
        g.installed = true;
        g
    }

    #[test]
    fn each_source_goes_through_its_platform() {
        let steam = game(GameSource::Steam, "620", Some("steam://rungameid/620"), None);
        assert_eq!(plan(&steam, false), UninstallPlan::Uri("steam://uninstall/620".into()));
        let leg = game(GameSource::Epic, "Fortnite", Some("legendary://Fortnite"), None);
        assert_eq!(
            plan(&leg, false),
            UninstallPlan::Command { program: "legendary".into(), args: vec!["uninstall".into(), "Fortnite".into()], stdin: Some("y\n".into()) }
        );
        let heroic = game(GameSource::Gog, "1207664663", Some("heroic://launch?appName=1207664663&runner=gog"), None);
        assert_eq!(plan(&heroic, false), UninstallPlan::Delegate { uri: Some("heroic://".into()), launcher: "Heroic".into() });
        let galaxy = game(GameSource::Gog, "1207664663", None, Some("C:/GOG/Game"));
        assert!(matches!(plan(&galaxy, true), UninstallPlan::Delegate { uri: Some(u), .. } if u == "goggalaxy://openGameView/1207664663"));
        let ubi = game(GameSource::Ubisoft, "635", Some("uplay://launch/635/0"), None);
        assert_eq!(plan(&ubi, true), UninstallPlan::Delegate { uri: Some("ms-settings:appsfeatures".into()), launcher: "Ubisoft Connect".into() });
        assert!(matches!(plan(&ubi, false), UninstallPlan::Delegate { uri: None, .. }));
        let solo = game(GameSource::Standalone, "x", None, Some("/home/a/Jeux/Celeste"));
        assert_eq!(plan(&solo, false), UninstallPlan::DeleteFolder("/home/a/Jeux/Celeste".into()));
    }

    #[test]
    fn roms_and_manual_games_are_never_deleted() {
        let rom = game(GameSource::Emulated, "/roms/snes/a.sfc", None, Some("/roms/snes"));
        assert_eq!(plan(&rom, false), UninstallPlan::HideFromLibrary);
        let manual = game(GameSource::Manual, "/opt/x/x.sh", None, Some("/opt/x"));
        assert_eq!(plan(&manual, false), UninstallPlan::RemoveFromLibrary);
        let mut steam = game(GameSource::Steam, "620", None, None);
        steam.installed = false;
        assert_eq!(plan(&steam, false), UninstallPlan::Unavailable(UnavailableReason::NotInstalled));
        assert_eq!(plan(&game(GameSource::Minecraft, "mc", None, None), false), UninstallPlan::Unavailable(UnavailableReason::Minecraft));
    }

    #[test]
    fn suspicious_legendary_names_are_not_run() {
        let bad = game(GameSource::Epic, "x", Some("legendary://--import"), None);
        assert!(matches!(plan(&bad, false), UninstallPlan::Delegate { .. }));
        let steam = game(GameSource::Steam, "62 0; rm", None, None);
        assert!(matches!(plan(&steam, false), UninstallPlan::Delegate { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn path_guards() {
        let reg = vec![PathBuf::from("/home/a/Jeux"), PathBuf::from("/mnt/disk/Games")];
        let protected = vec![PathBuf::from("/home/a"), PathBuf::from("/home/a/.local/share/Halcyon"), PathBuf::from("/usr")];
        let others = vec![PathBuf::from("/home/a/Jeux/Celeste/roms"), PathBuf::from("/mnt/disk/Games/Shared")];
        let ok = |p: &str| check_deletable(Path::new(p), &reg, &protected, &others);
        assert_eq!(ok("/home/a/Jeux/Hades"), Ok(()));
        assert_eq!(ok("/mnt/disk/Games/Tunic"), Ok(()));
        // Le dossier enregistré lui-même, un parent, la racine : jamais.
        assert_eq!(ok("/home/a/Jeux"), Err(Refusal::NotRegistered));
        assert_eq!(ok("/home/a"), Err(Refusal::Protected("/home/a".into())));
        assert_eq!(ok("/"), Err(Refusal::TooShallow));
        assert_eq!(ok("/home"), Err(Refusal::TooShallow));
        // Trop profond, ou hors des dossiers enregistrés.
        assert_eq!(ok("/home/a/Jeux/Hades/bin"), Err(Refusal::NotRegistered));
        assert_eq!(ok("/opt/Hades"), Err(Refusal::NotRegistered));
        // Chemins détournés.
        assert_eq!(ok("/home/a/Jeux/../Documents"), Err(Refusal::NotAbsolute));
        assert_eq!(ok("Jeux/Hades"), Err(Refusal::NotAbsolute));
        // Dossier qui contient une bibliothèque de ROM ou un autre jeu.
        assert_eq!(ok("/home/a/Jeux/Celeste"), Err(Refusal::Shared("/home/a/Jeux/Celeste/roms".into())));
        assert_eq!(ok("/mnt/disk/Games/Shared"), Err(Refusal::Shared("/mnt/disk/Games/Shared".into())));
        // Casse : distincte sous Unix.
        assert_eq!(ok("/home/a/jeux/Hades"), Err(Refusal::NotRegistered));
        // Dossier enregistré à la racine d'un disque : le jeu reste supprimable, pas le disque.
        let reg2 = vec![PathBuf::from("/mnt")];
        assert_eq!(check_deletable(Path::new("/mnt/Game"), &reg2, &[], &[]), Ok(()));
        assert_eq!(check_deletable(Path::new("/mnt"), &reg2, &[], &[]), Err(Refusal::TooShallow));
    }
}
