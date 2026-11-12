//! Modèle de domaine commun à toutes les sources de jeux.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::util;

/// Identifiant interne (clé primaire SQLite).
pub type GameId = i64;

/// Origine d'un jeu. La paire `(source, source_key)` est unique dans la bibliothèque.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GameSource {
    Steam,
    Epic,
    /// Détecté automatiquement dans un dossier surveillé.
    Standalone,
    /// Ajouté à la main par l'utilisateur.
    Manual,
    /// ROM lancée par un émulateur.
    Emulated,
    /// Minecraft Java Edition, géré par l'onglet Minecraft (0.5.0).
    Minecraft,
}

impl GameSource {
    pub const ALL: [GameSource; 6] = [Self::Steam, Self::Epic, Self::Standalone, Self::Manual, Self::Emulated, Self::Minecraft];

    /// Identifiant stable stocké en base.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Steam => "steam",
            Self::Epic => "epic",
            Self::Standalone => "standalone",
            Self::Manual => "manual",
            Self::Emulated => "emulated",
            Self::Minecraft => "minecraft",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }

    /// Libellé affiché dans l'interface.
    pub fn label(self) -> &'static str {
        match self {
            Self::Steam => "Steam",
            Self::Epic => "Epic Games",
            Self::Standalone => "Standalone",
            Self::Manual => "Ajout manuel",
            Self::Emulated => "Émulation",
            Self::Minecraft => "Minecraft",
        }
    }

    /// Les jeux de cette source se lancent-ils via le client de la plateforme ?
    pub fn uses_platform_client(self) -> bool {
        matches!(self, Self::Steam | Self::Epic)
    }
}

/// Champs modifiés par l'utilisateur : une synchronisation ne les écrase jamais.
pub mod overrides {
    pub const NAME: u32 = 1 << 0;
    pub const DESCRIPTION: u32 = 1 << 1;
    pub const COVER: u32 = 1 << 2;
    pub const BANNER: u32 = 1 << 3;
    pub const LAUNCH: u32 = 1 << 4;
    /// Développeur, éditeur, date de sortie, genres.
    pub const META: u32 = 1 << 5;
}

/// Statut de progression choisi par le joueur.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlayStatus {
    #[default]
    None,
    Backlog,
    Playing,
    Completed,
    Abandoned,
}

impl PlayStatus {
    pub const ALL: [PlayStatus; 5] = [Self::None, Self::Backlog, Self::Playing, Self::Completed, Self::Abandoned];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Backlog => "backlog",
            Self::Playing => "playing",
            Self::Completed => "completed",
            Self::Abandoned => "abandoned",
        }
    }

    pub fn parse(s: &str) -> Self {
        Self::ALL.into_iter().find(|v| v.as_str() == s).unwrap_or_default()
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "Aucun statut",
            Self::Backlog => "À jouer",
            Self::Playing => "En cours",
            Self::Completed => "Terminé",
            Self::Abandoned => "Abandonné",
        }
    }
}

/// État de mise à jour tel que rapporté par la plateforme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UpdateState {
    /// À jour, ou information non fournie par la plateforme.
    #[default]
    None,
    /// Une mise à jour est requise et n'a pas commencé.
    Available,
    /// Téléchargement ou installation en cours dans le client de la plateforme.
    Downloading,
    /// Mise à jour suspendue dans le client.
    Paused,
}

impl UpdateState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Available => "available",
            Self::Downloading => "downloading",
            Self::Paused => "paused",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "available" => Self::Available,
            "downloading" => Self::Downloading,
            "paused" => Self::Paused,
            _ => Self::None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "À jour",
            Self::Available => "Mise à jour disponible",
            Self::Downloading => "Téléchargement en cours",
            Self::Paused => "Mise à jour en pause",
        }
    }

    pub fn pending(self) -> bool {
        self != Self::None
    }
}

/// Mise à jour remontée par un scanner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub state: UpdateState,
    /// Octets déjà téléchargés / à télécharger (0 si inconnu).
    pub downloaded: u64,
    pub total: u64,
}

/// Visuels d'un jeu : chemins locaux (affichables hors ligne) et URL distantes
/// (téléchargées en tâche de fond quand le réseau est disponible).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Artwork {
    /// Jaquette verticale (ratio 2:3).
    pub cover: Option<PathBuf>,
    /// Bannière horizontale large (fiche du jeu, accueil).
    pub banner: Option<PathBuf>,
    /// Logo transparent (affiché par-dessus la bannière).
    pub logo: Option<PathBuf>,
    pub cover_url: Option<String>,
    pub banner_url: Option<String>,
    pub logo_url: Option<String>,
}

/// Manière de lancer un jeu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchTarget {
    /// URI gérée par le client de la plateforme (`steam://…`, `com.epicgames.launcher://…`).
    Uri(String),
    /// Exécutable choisi par l'utilisateur ; `args` est une ligne d'arguments à découper.
    Executable { path: PathBuf, args: String, working_dir: Option<PathBuf> },
    /// Programme et arguments déjà découpés (émulateurs, outils en ligne de commande).
    Process { program: PathBuf, args: Vec<String>, working_dir: Option<PathBuf> },
}

/// Préfixe interne des jeux Epic installés via Legendary (sans Heroic) sous Linux.
pub const LEGENDARY_URI: &str = "legendary://";

/// Un jeu de la bibliothèque, tel que stocké localement.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Game {
    pub id: GameId,
    pub source: GameSource,
    pub source_key: String,
    pub name: String,
    pub description: Option<String>,
    pub developer: Option<String>,
    pub publisher: Option<String>,
    /// Date de sortie au format libre fourni par la source (ex. « 2023-02-10 »).
    pub release_date: Option<String>,
    pub genres: Vec<String>,
    pub install_dir: Option<PathBuf>,
    pub executable: Option<PathBuf>,
    pub arguments: String,
    pub working_dir: Option<PathBuf>,
    pub launch_uri: Option<String>,
    pub installed: bool,
    pub favorite: bool,
    pub hidden: bool,
    pub artwork: Artwork,
    /// URL de captures d'écran (métadonnées en ligne).
    pub screenshots: Vec<String>,
    /// Plateforme émulée (identifiant de `platforms.json`), pour les ROM.
    pub platform: Option<String>,
    /// Émulateur imposé pour ce jeu (sinon celui par défaut de la plateforme).
    pub emulator: Option<String>,
    /// Identifiant RetroAchievements associé (calculé par empreinte ou saisi).
    pub ra_game_id: Option<u32>,
    /// Horodatage Unix (secondes) de l'ajout à la bibliothèque.
    pub added_at: i64,
    /// Horodatage Unix du dernier lancement connu.
    pub last_played: Option<i64>,
    pub playtime_secs: u64,
    pub play_count: u32,
    pub size_bytes: Option<u64>,
    /// Masque de bits [`overrides`].
    pub overrides: u32,
    pub status: PlayStatus,
    /// Date à laquelle le jeu a été marqué terminé.
    pub completed_at: Option<i64>,
    /// Notes libres du joueur.
    pub notes: String,
    /// Dernier temps de jeu rapporté par la plateforme (secondes), s'il existe.
    pub playtime_platform_secs: Option<u64>,
    /// Correction manuelle : remplace le temps calculé quand elle est définie.
    pub playtime_manual_secs: Option<u64>,
    /// Étiquettes (« multijoueur », « coop »…), issues de la boutique ou du joueur.
    pub tags: Vec<String>,
    /// Version installée (build Steam, version Epic…), si la plateforme la fournit.
    pub version: Option<String>,
    pub update: UpdateInfo,
    /// Première détection de l'installation (horodatage Unix).
    pub installed_at: Option<i64>,
    /// Clé de tri dérivée du nom (non persistée).
    #[serde(skip)]
    pub sort_key: String,
    /// Clé de recherche dérivée du nom (non persistée).
    #[serde(skip)]
    pub search_key: String,
}

impl Game {
    /// Jeu vide (tests, outils).
    pub fn blank(id: GameId, name: &str, source: GameSource) -> Self {
        let mut g = Game {
            id,
            source,
            source_key: id.to_string(),
            name: name.into(),
            description: None,
            developer: None,
            publisher: None,
            release_date: None,
            genres: vec![],
            install_dir: None,
            executable: None,
            arguments: String::new(),
            working_dir: None,
            launch_uri: None,
            installed: true,
            favorite: false,
            hidden: false,
            artwork: Artwork::default(),
            screenshots: vec![],
            platform: None,
            emulator: None,
            ra_game_id: None,
            added_at: id,
            last_played: None,
            playtime_secs: 0,
            play_count: 0,
            size_bytes: None,
            overrides: 0,
            status: PlayStatus::None,
            completed_at: None,
            notes: String::new(),
            playtime_platform_secs: None,
            playtime_manual_secs: None,
            tags: vec![],
            version: None,
            update: UpdateInfo::default(),
            installed_at: None,
            sort_key: String::new(),
            search_key: String::new(),
        };
        g.refresh_derived();
        g
    }

    /// Recalcule les champs dérivés (tri, recherche). À appeler après toute
    /// modification du nom.
    pub fn refresh_derived(&mut self) {
        self.sort_key = util::sort_key(&self.name);
        self.search_key = util::search_key(&self.name);
    }

    /// Détermine comment lancer ce jeu, hors émulation (voir le module émulateurs),
    /// ou `None` s'il n'est pas lançable.
    pub fn launch_target(&self) -> Option<LaunchTarget> {
        if self.source == GameSource::Emulated {
            return None;
        }
        // Un exécutable défini par l'utilisateur prime sur l'URI de plateforme.
        if self.overrides & overrides::LAUNCH != 0 || !self.source.uses_platform_client() {
            if let Some(path) = &self.executable {
                return Some(LaunchTarget::Executable {
                    path: path.clone(),
                    args: self.arguments.clone(),
                    working_dir: self.working_dir.clone().or_else(|| path.parent().map(|p| p.to_path_buf())),
                });
            }
        }
        let uri = self.launch_uri.clone()?;
        if let Some(app) = uri.strip_prefix(LEGENDARY_URI) {
            return Some(LaunchTarget::Process {
                program: PathBuf::from("legendary"),
                args: vec!["launch".into(), app.to_string()],
                working_dir: None,
            });
        }
        Some(LaunchTarget::Uri(uri))
    }

    /// Temps de jeu retenu : correction manuelle, sinon le maximum entre la
    /// plateforme et le suivi local.
    pub fn effective_playtime(&self) -> u64 {
        self.playtime_manual_secs.unwrap_or(self.playtime_secs)
    }

    /// Origine du temps de jeu affiché.
    pub fn playtime_origin(&self) -> PlaytimeOrigin {
        if self.playtime_manual_secs.is_some() {
            PlaytimeOrigin::Manual
        } else if self.playtime_platform_secs.is_some_and(|p| p > 0 && p >= self.playtime_secs) {
            PlaytimeOrigin::Platform
        } else if self.playtime_secs > 0 {
            PlaytimeOrigin::Local
        } else {
            PlaytimeOrigin::None
        }
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
    }

    pub fn has_override(&self, flag: u32) -> bool {
        self.overrides & flag != 0
    }
}

/// D'où vient le temps de jeu affiché (les statistiques distinguent ces cas).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaytimeOrigin {
    None,
    /// Rapporté par la plateforme (Steam…).
    Platform,
    /// Mesuré par Halcyon pendant les sessions.
    Local,
    /// Saisi par le joueur.
    Manual,
}

impl PlaytimeOrigin {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Platform => "plateforme",
            Self::Local => "suivi local",
            Self::Manual => "corrigé",
        }
    }
}

/// Jeu tel que remonté par un scanner, avant fusion dans la bibliothèque.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DiscoveredGame {
    pub source_key: String,
    pub name: String,
    pub description: Option<String>,
    pub developer: Option<String>,
    pub publisher: Option<String>,
    pub release_date: Option<String>,
    pub genres: Vec<String>,
    pub install_dir: Option<PathBuf>,
    pub executable: Option<PathBuf>,
    pub arguments: String,
    pub working_dir: Option<PathBuf>,
    pub launch_uri: Option<String>,
    pub installed: bool,
    pub artwork: Artwork,
    pub platform: Option<String>,
    /// Dernier lancement selon la plateforme (ex. Steam), fusionné avec nos données.
    pub last_played: Option<i64>,
    /// Temps de jeu selon la plateforme (secondes).
    pub playtime_secs: Option<u64>,
    pub size_bytes: Option<u64>,
    /// Version installée selon la plateforme.
    pub version: Option<String>,
    /// État de mise à jour, `None` si la plateforme ne l'expose pas.
    pub update: Option<UpdateInfo>,
}

impl DiscoveredGame {
    pub fn new(source_key: impl Into<String>, name: impl Into<String>) -> Self {
        Self { source_key: source_key.into(), name: name.into(), installed: true, ..Default::default() }
    }
}

/// Saisie de l'utilisateur pour ajouter un jeu à la main.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ManualGameInput {
    pub name: String,
    pub executable: PathBuf,
    pub arguments: String,
    pub working_dir: Option<PathBuf>,
}

/// Modification des paramètres de lancement depuis la fiche d'un jeu.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LaunchSettingsUpdate {
    pub name: String,
    pub executable: Option<PathBuf>,
    pub arguments: String,
    pub working_dir: Option<PathBuf>,
}

/// Modification manuelle des métadonnées.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MetadataUpdate {
    pub description: String,
    pub developer: String,
    pub publisher: String,
    pub release_date: String,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
}

/// Métadonnées récupérées en ligne (ex. boutique Steam).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OnlineMetadata {
    pub description: Option<String>,
    pub developer: Option<String>,
    pub publisher: Option<String>,
    pub release_date: Option<String>,
    pub genres: Vec<String>,
    pub screenshots: Vec<String>,
    pub tags: Vec<String>,
}

/// Collection personnalisée.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub count: usize,
}
