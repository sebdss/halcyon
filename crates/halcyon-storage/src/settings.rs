//! Configuration persistante (`settings.json`).
//!
//! Écriture atomique (fichier temporaire + renommage) : une fermeture brutale
//! ne laisse jamais un fichier à moitié écrit. Un fichier illisible est mis de
//! côté (`.bak`) et remplacé par les valeurs par défaut, sans bloquer le démarrage.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Champs de [`Settings`] conservés dans le coffre des secrets, jamais dans le JSON.
pub const SECRET_FIELDS: &[&str] = &["steam_api_key", "ra_api_key", "nexus_api_key", "steamgriddb_api_key", "curseforge_api_key"];

/// Coffre voisin du fichier de configuration (`<données>/secrets`).
pub fn secret_store(settings_path: &Path) -> crate::secrets::SecretStore {
    crate::secrets::SecretStore::new(settings_path.parent().unwrap_or(Path::new(".")).join("secrets"))
}

/// Configuration d'un émulateur (profil).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EmulatorSettings {
    /// Identifiant de la définition (`retroarch`, `dolphin`…).
    pub id: String,
    /// Exécutable forcé (sinon détection automatique).
    pub path: Option<PathBuf>,
    /// Arguments ajoutés avant ceux de la définition.
    pub extra_args: String,
    pub fullscreen: bool,
    pub enabled: bool,
}

/// Dossier de ROM associé à une plateforme.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RomFolder {
    pub platform: String,
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub steam_enabled: bool,
    /// Dossier Steam forcé (sinon détection automatique).
    pub steam_path: Option<PathBuf>,
    pub epic_enabled: bool,
    /// GOG (Heroic, GOG Galaxy), 1.0.0.
    pub gog_enabled: bool,
    /// Dossier de configuration Heroic forcé (sinon détection automatique).
    pub gog_heroic_path: Option<PathBuf>,
    /// Dossier des manifestes Epic forcé (sinon détection automatique).
    pub epic_manifests_path: Option<PathBuf>,
    /// Dossiers parcourus à la recherche de jeux standalone.
    pub scan_folders: Vec<PathBuf>,
    /// Autorise le téléchargement des jaquettes manquantes (nécessite Internet).
    pub online_artwork: bool,
    /// Lance un scan des sources au démarrage (en arrière-plan).
    pub scan_on_startup: bool,
    /// Réduit la fenêtre au lancement d'un jeu (plus aucun rendu pendant la partie).
    pub minimize_on_launch: bool,
    pub library_view: LibraryView,
    pub library_sort: String,
    /// Taille des jaquettes dans la grille (largeur en pixels logiques).
    pub card_width: u32,
    pub window_width: u32,
    pub window_height: u32,
    pub window_maximized: bool,

    /// Linux : commande utilisée pour lancer les `.exe` (wine, umu-run, proton…).
    pub windows_runner: String,

    pub emulators: Vec<EmulatorSettings>,
    pub rom_folders: Vec<RomFolder>,
    /// Émulateur par défaut de chaque plateforme.
    pub platform_emulator: BTreeMap<String, String>,

    /// Clé Steam Web API personnelle (https://steamcommunity.com/dev/apikey).
    pub steam_api_key: String,
    /// SteamID64 forcé (sinon compte Steam le plus récent de cet ordinateur).
    pub steam_id: String,
    pub ra_user: String,
    pub ra_api_key: String,
    pub sync_achievements_on_startup: bool,
    /// Notifications système lors d'un déblocage.
    pub achievement_notifications: bool,
    /// Vérifie les nouveaux succès pendant une partie (requête périodique).
    pub achievement_polling: bool,
    /// Descriptions, genres et captures depuis la boutique Steam.
    pub online_metadata: bool,

    pub check_updates: bool,
    /// Dépôt GitHub « propriétaire/nom » publiant les versions.
    pub update_repo: String,
    pub start_in_big_picture: bool,
    /// Notification système quand une mise à jour de jeu est détectée.
    pub update_notifications: bool,

    /// Dépôt des sauvegardes de jeux (dossier local, disque secondaire, NAS) ;
    /// `None` : dans les données de Halcyon.
    pub save_root: Option<PathBuf>,
    /// Copies supplémentaires de chaque sauvegarde (autres disques).
    pub save_mirrors: Vec<PathBuf>,
    /// Sauvegardes automatiques conservées par jeu (les manuelles ne sont jamais purgées).
    pub save_keep_auto: u32,
    /// Nom de ce PC dans les sauvegardes partagées (vide : nom du système).
    pub machine_name: String,
    /// Clé d'API personnelle Nexus Mods (vérification des versions des mods).
    pub nexus_api_key: String,
    /// Clé d'API SteamGridDB (visuels des jeux standalone et ajoutés à la main).
    pub steamgriddb_api_key: String,
    /// Aucune requête réseau, même si une connexion est disponible.
    pub offline_forced: bool,
    /// Performance Center : résumé processeur / mémoire / carte graphique de chaque partie
    /// (un échantillon toutes les 10 s pendant le jeu).
    pub perf_history: bool,
    /// Gestionnaire de téléchargements : limite en Kio/s (0 : illimitée).
    pub download_limit_kib: u64,
    /// Téléchargements simultanés (les suivants attendent).
    pub download_concurrency: u32,
    /// Profils d'interface enregistrés (1.0.0).
    pub ui_profiles: Vec<UiProfile>,
    /// Dossier des journaux de temps d'image (MangoHud `output_folder`, PresentMon).
    pub frametime_dir: Option<PathBuf>,

    pub appearance: Appearance,
    /// Extensions activées (identifiants). Une extension installée reste inactive tant
    /// qu'elle n'a pas été activée explicitement.
    pub plugins_enabled: Vec<String>,
    /// Profils de manette (touches de navigation, options des émulateurs).
    pub pad_profiles: Vec<PadProfile>,
    /// Profil de manette actif (0 : disposition standard).
    pub active_pad_profile: i64,
    /// Machines Sunshine connues (jeu à distance).
    pub remote_hosts: Vec<RemoteHost>,
    /// Exécutable Moonlight forcé (sinon détection automatique).
    pub moonlight_path: Option<PathBuf>,

    /// Lancement à l'ouverture de session (réduit dans la zone de notification).
    pub autostart: bool,
    /// Fermer la fenêtre garde Halcyon dans la zone de notification.
    pub close_to_tray: bool,
    /// Réduire la fenêtre la place dans la zone de notification.
    pub minimize_to_tray: bool,
    /// Raccourcis clavier personnalisés (action → combinaison, ex. `Ctrl+K`).
    pub shortcuts: BTreeMap<String, String>,
    /// Raccourci global (hors de Halcyon) ouvrant le lancement rapide.
    pub global_hotkey_enabled: bool,
    pub global_hotkey: String,
    /// Petite fenêtre au premier plan pendant les parties (temps, succès, notes).
    pub companion_window: bool,
    /// Effets sonores (0.4.1).
    pub sound: SoundSettings,
    /// Minecraft Java Edition (0.5.0).
    pub minecraft: MinecraftSettings,
    /// Clé d'API CurseForge (mods et modpacks CurseForge). Secret : exclu des exports.
    pub curseforge_api_key: String,
}

/// Effets sonores. Volumes de 0 à 1, indépendants du volume du système.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundSettings {
    pub enabled: bool,
    /// Mode silencieux : coupe tout sans perdre les réglages.
    pub muted: bool,
    pub master: f32,
    pub navigation: f32,
    pub interaction: f32,
    pub notification: f32,
    pub important: f32,
    /// Sons de survol (désactivés par défaut).
    pub hover: bool,
    /// Pendant une partie lancée par Halcyon : 0 silencieux, 1 réduit, 2 normal.
    pub in_game: u8,
}

impl Default for SoundSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            muted: false,
            master: 0.7,
            navigation: 0.8,
            interaction: 0.9,
            notification: 0.9,
            important: 1.0,
            hover: false,
            in_game: 0,
        }
    }
}

impl SoundSettings {
    fn sanitize(&mut self) {
        for v in [&mut self.master, &mut self.navigation, &mut self.interaction, &mut self.notification, &mut self.important] {
            *v = if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.7 };
        }
        self.in_game = self.in_game.min(2);
    }
}

/// Profil d'interface nommé (1.0.0) : une apparence complète enregistrée (« Bureau »,
/// « Salon », « Portable sur batterie »…), appliquée d'un clic.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiProfile {
    pub name: String,
    pub appearance: Appearance,
}

/// Personnalisation de l'interface.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    /// Thème intégré (`halcyon`, `minuit`…), importé ou fourni par une extension.
    pub theme: String,
    /// Couleur d'accent « #rrggbb » ; vide : celle du thème.
    pub accent: String,
    /// 0 : confortable, 1 : compacte.
    pub density: u8,
    pub animations: bool,
    pub background: Option<PathBuf>,
    pub background_opacity: f32,
    pub animated_background: bool,
    /// Entrées de la barre latérale masquées (`stats`, `journal`…).
    pub hidden_nav: Vec<String>,
    /// Sections de l'accueil dans l'ordre d'affichage (codes 0–6), avec visibilité.
    /// Format 0.3, conservé pour la compatibilité : la 0.4 utilise `widgets`.
    pub home_sections: Vec<HomeSection>,

    // ---- 0.4 ----
    /// 0 standard, 1 minimaliste, 2 cinéma.
    pub ui_mode: u8,
    pub sidebar_collapsed: bool,
    /// Mouvement : 0 complet, 1 réduit, 2 aucun (remplace `animations`, conservé).
    pub motion: u8,
    /// Taille du texte (accessibilité), 0,9 à 1,3.
    pub text_scale: f32,
    /// Vue de la bibliothèque : 0 grille, 1 liste, 2 carrousel, 3 immersive, 4 compacte.
    pub library_view: u8,
    /// Widgets de l'accueil dans l'ordre (vide : migration depuis `home_sections` ou défaut).
    pub widgets: Vec<WidgetConf>,
    /// Mode Performance activé à la main.
    pub performance: bool,
    /// Mode Performance automatique pendant les parties lancées par Halcyon.
    pub performance_auto: bool,
}

/// Widget de l'accueil : type, largeur (1 à 3 colonnes sur 3), affichage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WidgetConf {
    pub kind: u8,
    pub span: u8,
    pub enabled: bool,
}

/// Types de widgets (l'ordre des constantes est celui de l'enregistrement).
pub mod widget {
    pub const CONTINUE: u8 = 0;
    pub const RECENT: u8 = 1;
    pub const FAVORITES: u8 = 2;
    pub const LIBRARY: u8 = 3;
    pub const ACHIEVEMENTS: u8 = 4;
    pub const STATS: u8 = 5;
    pub const PLAYTIME: u8 = 6;
    pub const ACTIVITY: u8 = 7;
    pub const STORAGE: u8 = 8;
    pub const EMULATORS: u8 = 9;
    pub const CAPTURES: u8 = 10;
    pub const SUGGESTIONS: u8 = 11;
    pub const ADDED: u8 = 12;
    pub const PLAYING: u8 = 13;
    pub const COUNT: u8 = 14;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomeSection {
    pub code: u8,
    pub visible: bool,
}

impl Appearance {
    pub const SECTION_COUNT: u8 = 7;

    /// Disposition par défaut de la 0.4.
    pub fn default_widgets() -> Vec<WidgetConf> {
        use widget::*;
        let on = |kind, span| WidgetConf { kind, span, enabled: true };
        let off = |kind, span| WidgetConf { kind, span, enabled: false };
        vec![
            on(CONTINUE, 3),
            on(RECENT, 3),
            on(PLAYTIME, 1),
            on(STATS, 2),
            on(FAVORITES, 3),
            on(ACHIEVEMENTS, 2),
            on(LIBRARY, 1),
            on(SUGGESTIONS, 3),
            on(ACTIVITY, 2),
            on(STORAGE, 1),
            off(CAPTURES, 3),
            off(EMULATORS, 1),
            off(ADDED, 3),
            off(PLAYING, 3),
        ]
    }

    /// Widgets dans l'ordre, complétés et nettoyés. Sans configuration 0.4, les sections de
    /// l'accueil de la 0.3 sont reprises (ordre et visibilité), puis les nouveaux widgets
    /// sont ajoutés désactivés.
    pub fn widgets(&self) -> Vec<WidgetConf> {
        use widget::*;
        let mut out: Vec<WidgetConf> = Vec::new();
        let source: Vec<WidgetConf> = if !self.widgets.is_empty() {
            self.widgets.clone()
        } else if !self.home_sections.is_empty() {
            self.sections()
                .iter()
                .map(|s| {
                    let (kind, span) = match s.code {
                        0 => (CONTINUE, 3),
                        1 => (LIBRARY, 1),
                        2 => (RECENT, 3),
                        3 => (FAVORITES, 3),
                        4 => (ADDED, 3),
                        5 => (PLAYING, 3),
                        _ => (SUGGESTIONS, 3),
                    };
                    WidgetConf { kind, span, enabled: s.visible }
                })
                .collect()
        } else {
            return Self::default_widgets();
        };
        for w in source {
            if w.kind < COUNT && !out.iter().any(|o| o.kind == w.kind) {
                out.push(WidgetConf { span: w.span.clamp(1, 3), ..w });
            }
        }
        for d in Self::default_widgets() {
            if !out.iter().any(|o| o.kind == d.kind) {
                out.push(WidgetConf { enabled: false, ..d });
            }
        }
        out
    }

    /// Mouvement effectif (les réglages 0.3 sans animations valent « aucun »).
    pub fn motion(&self) -> u8 {
        if !self.animations { 2 } else { self.motion.min(2) }
    }

    /// Ordre complet : sections connues manquantes ajoutées (masquées si nouvelles), doublons retirés.
    pub fn sections(&self) -> Vec<HomeSection> {
        let mut out: Vec<HomeSection> = Vec::new();
        for s in &self.home_sections {
            if s.code < Self::SECTION_COUNT && !out.iter().any(|o| o.code == s.code) {
                out.push(*s);
            }
        }
        for code in 0..Self::SECTION_COUNT {
            if !out.iter().any(|o| o.code == code) {
                out.push(HomeSection { code, visible: self.home_sections.is_empty() && code < 5 });
            }
        }
        out
    }
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: "halcyon".into(),
            accent: String::new(),
            density: 0,
            animations: true,
            background: None,
            background_opacity: 0.15,
            animated_background: false,
            hidden_nav: Vec::new(),
            home_sections: Vec::new(),
            ui_mode: 0,
            sidebar_collapsed: false,
            motion: 0,
            text_scale: 1.0,
            library_view: 0,
            widgets: Vec::new(),
            performance: false,
            performance_auto: true,
        }
    }
}

/// Profil de manette : boutons associés aux actions de navigation et options
/// transmises aux émulateurs (aucune interception pendant les jeux PC).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PadProfile {
    pub id: i64,
    pub name: String,
    /// Action (`accept`, `back`…) → bouton (`South`, `East`…). Absente : disposition standard.
    pub mapping: BTreeMap<String, String>,
    /// Identifiant d'émulateur → arguments ajoutés avant la ROM.
    pub emulator_args: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RemoteHost {
    pub name: String,
    pub address: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryView {
    #[default]
    Grid,
    List,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            steam_enabled: true,
            steam_path: None,
            epic_enabled: true,
            gog_enabled: true,
            gog_heroic_path: None,
            epic_manifests_path: None,
            scan_folders: Vec::new(),
            online_artwork: true,
            scan_on_startup: true,
            minimize_on_launch: true,
            library_view: LibraryView::Grid,
            library_sort: "name".into(),
            card_width: 168,
            window_width: 1440,
            window_height: 900,
            window_maximized: false,
            windows_runner: "wine".into(),
            emulators: Vec::new(),
            rom_folders: Vec::new(),
            platform_emulator: BTreeMap::new(),
            steam_api_key: String::new(),
            steam_id: String::new(),
            ra_user: String::new(),
            ra_api_key: String::new(),
            sync_achievements_on_startup: true,
            achievement_notifications: true,
            achievement_polling: true,
            online_metadata: true,
            check_updates: true,
            update_repo: String::new(),
            start_in_big_picture: false,
            update_notifications: true,
            save_root: None,
            save_mirrors: Vec::new(),
            save_keep_auto: 20,
            machine_name: String::new(),
            nexus_api_key: String::new(),
            steamgriddb_api_key: String::new(),
            offline_forced: false,
            appearance: Appearance::default(),
            plugins_enabled: Vec::new(),
            pad_profiles: Vec::new(),
            active_pad_profile: 0,
            remote_hosts: Vec::new(),
            moonlight_path: None,
            autostart: false,
            close_to_tray: false,
            minimize_to_tray: false,
            shortcuts: BTreeMap::new(),
            global_hotkey_enabled: false,
            global_hotkey: "Ctrl+Alt+H".into(),
            companion_window: false,
            sound: SoundSettings::default(),
            minecraft: MinecraftSettings::default(),
            curseforge_api_key: String::new(),
            perf_history: true,
            download_limit_kib: 0,
            download_concurrency: 2,
            ui_profiles: Vec::new(),
            frametime_dir: None,
        }
    }
}

impl Settings {
    /// Charge la configuration ; ne renvoie jamais d'erreur bloquante.
    ///
    /// Les clés d'API sont lues dans le coffre `secrets/` voisin (voir [`crate::secrets`]).
    /// Une clé encore présente en clair dans le fichier (version ≤ 0.5) est déplacée dans le
    /// coffre, puis le fichier est réécrit sans elle.
    pub fn load(path: &Path) -> Self {
        let mut s = Self::load_file(path);
        let vault = secret_store(path);
        let mut migrated = false;
        for &name in SECRET_FIELDS {
            let field = s.secret_mut(name);
            if !field.trim().is_empty() {
                match vault.set(name, field) {
                    Ok(_) => migrated = true,
                    Err(e) => log::error!("coffre des secrets : {name} non déplacée ({e})"),
                }
            } else if let Some(v) = vault.get(name) {
                *field = v;
            }
        }
        if migrated {
            log::info!("clés d'API déplacées de settings.json vers le coffre des secrets");
            crate::audit::event("secret.migrated", "settings.json → secrets/");
            if let Err(e) = s.save(path) {
                log::error!("réécriture de la configuration sans secrets impossible : {e}");
            }
        }
        s
    }

    fn load_file(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => match serde_json::from_str::<Settings>(&text) {
                Ok(mut s) => {
                    s.sanitize();
                    s
                }
                Err(e) => {
                    log::error!("configuration illisible ({e}), sauvegarde en .bak et valeurs par défaut");
                    let _ = std::fs::rename(path, path.with_extension("json.bak"));
                    Self::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => {
                log::error!("lecture de la configuration impossible : {e}");
                Self::default()
            }
        }
    }

    /// Enregistre de manière atomique, fichier réservé à l'utilisateur (Unix : `0600`).
    /// Les clés d'API vont dans le coffre ; le fichier JSON n'en contient aucune.
    pub fn save(&self, path: &Path) -> crate::Result<()> {
        let vault = secret_store(path);
        let mut clean = self.clone();
        for &name in SECRET_FIELDS {
            let field = clean.secret_mut(name);
            vault.set(name, field)?;
            field.clear();
        }
        let data = serde_json::to_vec_pretty(&clean)?;
        crate::secrets::write_private(path, &data)?;
        Ok(())
    }

    fn secret_mut(&mut self, name: &str) -> &mut String {
        match name {
            "steam_api_key" => &mut self.steam_api_key,
            "ra_api_key" => &mut self.ra_api_key,
            "nexus_api_key" => &mut self.nexus_api_key,
            "steamgriddb_api_key" => &mut self.steamgriddb_api_key,
            "curseforge_api_key" => &mut self.curseforge_api_key,
            _ => unreachable!("champ secret inconnu : {name}"),
        }
    }

    fn sanitize(&mut self) {
        self.card_width = self.card_width.clamp(120, 260);
        self.sound.sanitize();
        self.minecraft.sanitize();
        self.window_width = self.window_width.clamp(960, 7680);
        self.window_height = self.window_height.clamp(600, 4320);
        let mut seen = std::collections::HashSet::new();
        self.scan_folders.retain(|p| seen.insert(halcyon_core::util::path_key(p)));
        let mut seen = std::collections::HashSet::new();
        self.rom_folders.retain(|f| seen.insert((f.platform.clone(), halcyon_core::util::path_key(&f.path))));
        if self.windows_runner.trim().is_empty() {
            self.windows_runner = "wine".into();
        }
    }

    /// Enregistre l'apparence actuelle sous `name` (remplace un profil du même nom, sans
    /// tenir compte de la casse). Renvoie `false` si le nom est vide.
    pub fn save_ui_profile(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let p = UiProfile { name: name.chars().take(40).collect(), appearance: self.appearance.clone() };
        match self.ui_profiles.iter().position(|x| x.name.eq_ignore_ascii_case(name)) {
            Some(i) => self.ui_profiles[i] = p,
            None if self.ui_profiles.len() < 20 => self.ui_profiles.push(p),
            None => return false,
        }
        true
    }

    /// Applique un profil. Le mode Performance en cours (manuel ou automatique) est conservé :
    /// changer d'apparence ne doit pas réactiver d'effets pendant une partie.
    pub fn apply_ui_profile(&mut self, index: usize) -> bool {
        let Some(p) = self.ui_profiles.get(index) else { return false };
        let (perf, auto) = (self.appearance.performance, self.appearance.performance_auto);
        self.appearance = p.appearance.clone();
        self.appearance.performance = perf;
        self.appearance.performance_auto = auto;
        true
    }

    pub fn delete_ui_profile(&mut self, index: usize) -> bool {
        (index < self.ui_profiles.len()).then(|| self.ui_profiles.remove(index)).is_some()
    }

    /// Profil d'un émulateur (créé à la volée s'il n'existe pas encore).
    pub fn emulator_mut(&mut self, id: &str) -> &mut EmulatorSettings {
        if let Some(i) = self.emulators.iter().position(|e| e.id == id) {
            return &mut self.emulators[i];
        }
        self.emulators.push(EmulatorSettings { id: id.into(), enabled: true, ..Default::default() });
        self.emulators.last_mut().expect("élément ajouté")
    }

    pub fn emulator(&self, id: &str) -> Option<&EmulatorSettings> {
        self.emulators.iter().find(|e| e.id == id)
    }
}

/// Réglages de Minecraft.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MinecraftSettings {
    /// Identifiant d'application Azure (connexion directe). Vide : connexion par le launcher officiel.
    pub client_id: String,
    /// `official` : jeu lancé par le launcher officiel (profils ajoutés) ; `direct` : par Halcyon.
    pub play_mode: String,
    /// Accord donné pour ajouter des profils au launcher officiel.
    pub official_consent: bool,
    pub ram_min_mb: u32,
    pub ram_max_mb: u32,
    /// Vide : Java officiel de Mojang adapté à chaque version (téléchargé au besoin).
    pub java_path: String,
    pub jvm_args: String,
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    pub show_snapshots: bool,
    pub show_old: bool,
    /// Dernière installation jouée.
    pub last_profile: String,
}

impl Default for MinecraftSettings {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            play_mode: "official".into(),
            official_consent: false,
            ram_min_mb: 1024,
            ram_max_mb: 4096,
            java_path: String::new(),
            jvm_args: String::new(),
            width: 0,
            height: 0,
            fullscreen: false,
            show_snapshots: false,
            show_old: false,
            last_profile: String::new(),
        }
    }
}

impl MinecraftSettings {
    pub fn sanitize(&mut self) {
        self.ram_max_mb = self.ram_max_mb.clamp(512, 65_536);
        self.ram_min_mb = self.ram_min_mb.clamp(256, self.ram_max_mb);
        if self.play_mode != "direct" && self.play_mode != "official" {
            self.play_mode = "official".into();
        }
        self.width = self.width.min(16_384);
        self.height = self.height.min(16_384);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_profiles_save_apply_delete() {
        let mut s = Settings::default();
        s.appearance.density = 2;
        assert!(!s.save_ui_profile("  "));
        assert!(s.save_ui_profile("Salon"));
        s.appearance.density = 0;
        s.appearance.performance = true;
        assert!(s.save_ui_profile("salon"), "même nom : remplacé");
        assert_eq!(s.ui_profiles.len(), 1);
        s.appearance.density = 1;
        s.appearance.performance = false;
        assert!(s.apply_ui_profile(0));
        assert_eq!(s.appearance.density, 0);
        assert!(!s.appearance.performance, "le mode Performance courant est conservé");
        assert!(!s.apply_ui_profile(5));
        assert!(s.delete_ui_profile(0));
        assert!(s.ui_profiles.is_empty());
    }

    #[test]
    fn api_keys_live_in_the_vault_not_in_the_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        // Fichier 0.5 : clé en clair.
        std::fs::write(&path, r#"{"steam_api_key":"PLAIN-KEY","card_width":180}"#).unwrap();
        let s = Settings::load(&path);
        assert_eq!(s.steam_api_key, "PLAIN-KEY", "la clé reste utilisable");
        assert_eq!(s.card_width, 180);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("PLAIN-KEY"), "fichier réécrit sans la clé");
        assert_eq!(secret_store(&path).get("steam_api_key").as_deref(), Some("PLAIN-KEY"));

        // Nouvel enregistrement : toujours rien en clair, relecture identique.
        let s2 = Settings { curseforge_api_key: "CF".into(), ..s.clone() };
        s2.save(&path).unwrap();
        assert!(!std::fs::read_to_string(&path).unwrap().contains("CF\""));
        assert_eq!(Settings::load(&path), s2);

        // Clé effacée dans les paramètres : retirée du coffre.
        let s3 = Settings { steam_api_key: String::new(), ..s2 };
        s3.save(&path).unwrap();
        assert_eq!(secret_store(&path).get("steam_api_key"), None);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn roundtrip_and_corruption_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        assert_eq!(Settings::load(&path), Settings::default());

        let s = Settings { scan_folders: vec!["D:/Games".into()], card_width: 200, ..Default::default() };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);

        std::fs::write(&path, "{ pas du json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        assert!(path.with_extension("json.bak").exists());
    }

    #[test]
    fn missing_fields_take_defaults() {
        let s: Settings = serde_json::from_str(r#"{"steam_enabled": false}"#).unwrap();
        assert!(!s.steam_enabled);
        assert!(s.epic_enabled);
    }

    #[test]
    fn sound_settings_default_and_sanitized() {
        // Fichier 0.4.0 (sans bloc « sound ») : valeurs par défaut.
        let mut s: Settings = serde_json::from_str(r#"{"card_width":168}"#).unwrap();
        assert_eq!(s.sound, SoundSettings::default());
        assert!(s.sound.enabled && !s.sound.hover && s.sound.in_game == 0);
        s.sound.master = 7.0;
        s.sound.navigation = f32::NAN;
        s.sound.in_game = 9;
        s.sanitize();
        assert_eq!(s.sound.master, 1.0);
        assert_eq!(s.sound.navigation, 0.7);
        assert_eq!(s.sound.in_game, 2);
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.sound, s.sound);
    }

    #[test]
    fn appearance_from_03_migrates_home_sections() {
        // settings.json de la 0.3 : pas de champ 0.4, sections réordonnées, animations coupées.
        let json = r#"{"appearance": {"theme": "minuit", "animations": false,
            "home_sections": [{"code": 2, "visible": true}, {"code": 0, "visible": false}]}}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        let a = &s.appearance;
        assert_eq!(a.theme, "minuit");
        assert_eq!(a.motion(), 2);
        assert!(a.performance_auto);
        let w = a.widgets();
        assert_eq!(w[0], WidgetConf { kind: widget::RECENT, span: 3, enabled: true });
        assert_eq!(w[1], WidgetConf { kind: widget::CONTINUE, span: 3, enabled: false });
        assert_eq!(w.len() as u8, widget::COUNT);
        assert!(w.iter().filter(|x| x.kind == widget::CAPTURES).all(|x| !x.enabled));
        // Sans rien : disposition par défaut.
        assert_eq!(Appearance::default().widgets(), Appearance::default_widgets());
    }
}

