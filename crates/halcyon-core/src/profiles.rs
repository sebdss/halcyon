//! Profils de lancement : plusieurs façons de lancer un même jeu.
//!
//! Un profil ne modifie jamais les fichiers du jeu : il agit uniquement sur la
//! ligne de commande, l'environnement et le processus que Halcyon démarre. Ce qui
//! ne peut pas être appliqué pour un jeu donné est signalé, jamais simulé.

use crate::launch_args::split_args;
use crate::model::GameId;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayMode {
    #[default]
    Default,
    Windowed,
    Fullscreen,
    Borderless,
}

impl DisplayMode {
    pub const ALL: [DisplayMode; 4] = [Self::Default, Self::Windowed, Self::Fullscreen, Self::Borderless];
    pub fn label(self) -> String {
        match self {
            Self::Default => tr!("Réglage du jeu"),
            Self::Windowed => tr!("Fenêtré"),
            Self::Fullscreen => tr!("Plein écran"),
            Self::Borderless => tr!("Fenêtré sans bordure"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpuPreference {
    #[default]
    Default,
    /// Carte graphique dédiée (portables à double GPU).
    HighPerformance,
    /// GPU intégré, économie d'énergie.
    PowerSaving,
}

impl GpuPreference {
    pub const ALL: [GpuPreference; 3] = [Self::Default, Self::HighPerformance, Self::PowerSaving];
    pub fn label(self) -> String {
        match self {
            Self::Default => tr!("Choix du système"),
            Self::HighPerformance => tr!("Haute performance (GPU dédié)"),
            Self::PowerSaving => tr!("Économie d'énergie (GPU intégré)"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessPriority {
    Low,
    BelowNormal,
    #[default]
    Normal,
    AboveNormal,
    High,
}

impl ProcessPriority {
    pub const ALL: [ProcessPriority; 5] = [Self::Low, Self::BelowNormal, Self::Normal, Self::AboveNormal, Self::High];
    pub fn label(self) -> String {
        match self {
            Self::Low => tr!("Basse"),
            Self::BelowNormal => tr!("Inférieure à la normale"),
            Self::Normal => tr!("Normale"),
            Self::AboveNormal => tr!("Supérieure à la normale"),
            Self::High => tr!("Haute"),
        }
    }
    /// Valeur « nice » Unix équivalente.
    pub fn nice(self) -> i32 {
        match self {
            Self::Low => 15,
            Self::BelowNormal => 5,
            Self::Normal => 0,
            Self::AboveNormal => -5,
            Self::High => -10,
        }
    }
}

/// Programme lancé avec le jeu (outil de capture, chat vocal, MangoHud…).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Companion {
    pub path: PathBuf,
    pub args: String,
    /// Fermé par Halcyon à la fin de la session (seulement s'il a été démarré par Halcyon).
    pub close_after: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchProfile {
    pub id: i64,
    pub game_id: GameId,
    pub name: String,
    pub is_default: bool,
    /// Arguments ajoutés (ou remplaçant ceux du jeu si `replace_args`).
    pub args: String,
    pub replace_args: bool,
    pub env: Vec<(String, String)>,
    pub resolution: Option<(u32, u32)>,
    pub display: DisplayMode,
    pub fps_limit: Option<u32>,
    pub gpu: GpuPreference,
    /// Windows : autorisation explicite de modifier temporairement la préférence
    /// graphique du système pour l'exécutable (rétablie après la session).
    pub allow_gpu_registry: bool,
    pub priority: ProcessPriority,
    pub working_dir: Option<PathBuf>,
    /// Commande exécutée avant le lancement (attendue, 60 s maximum).
    pub pre_command: String,
    /// Commande exécutée à la fin de la session.
    pub post_command: String,
    pub companions: Vec<Companion>,
    /// Profil de mods à activer avant le lancement.
    pub mod_profile: Option<i64>,
    /// Profil de manette associé.
    pub controller_profile: Option<i64>,
}

/// Moteur détecté dans le dossier du jeu : détermine les options applicables.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Engine {
    #[default]
    Unknown,
    Unity,
    Unreal,
    Source,
    Godot,
}

impl Engine {
    pub fn label(self) -> String {
        match self {
            Self::Unknown => tr!("moteur non reconnu"),
            Self::Unity => "Unity".into(),
            Self::Unreal => "Unreal Engine".into(),
            Self::Source => "Source".into(),
            Self::Godot => "Godot".into(),
        }
    }
}

/// Façon dont le jeu est démarré, qui limite ce qu'un profil peut appliquer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchKind {
    /// Halcyon démarre le processus (exécutable, émulateur, Legendary).
    Direct,
    /// Via Steam : les arguments passent par `steam://run`, rien d'autre.
    Steam,
    /// Via un autre client (Epic…) : aucun argument transmissible.
    PlatformClient,
}

/// Contexte système pour construire le plan.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostInfo {
    pub windows: bool,
    /// Pilote NVIDIA propriétaire présent (Linux, délestage PRIME).
    pub nvidia: bool,
    /// Le jeu Windows passe par Wine/Proton (Linux).
    pub via_wine: bool,
}

/// Ce qu'un profil applique concrètement à un lancement.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProfilePlan {
    /// Arguments finaux (`None` : garder ceux du jeu).
    pub args: Option<Vec<String>>,
    pub env: Vec<(String, String)>,
    pub priority: Option<ProcessPriority>,
    pub working_dir: Option<PathBuf>,
    /// Windows : préférence graphique à poser temporairement.
    pub gpu_registry: Option<GpuPreference>,
    /// Réglages non applicables, expliqués au joueur.
    pub notes: Vec<String>,
}

impl LaunchProfile {
    pub fn new(game_id: GameId, name: &str) -> Self {
        Self { game_id, name: name.into(), ..Default::default() }
    }

    /// Arguments propres au moteur pour la résolution, l'affichage et la limite d'images.
    /// Les réglages sans équivalent pour ce moteur sont listés dans `notes`.
    pub fn engine_args(&self, engine: Engine, notes: &mut Vec<String>) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let unsupported = |what: &str, notes: &mut Vec<String>| {
            notes.push(tr!("{what} : non applicable automatiquement ({label}).", what = what, label = engine.label()));
        };
        if let Some((w, h)) = self.resolution {
            match engine {
                Engine::Unity => out.extend(["-screen-width".into(), w.to_string(), "-screen-height".into(), h.to_string()]),
                Engine::Unreal => out.extend([format!("-ResX={w}"), format!("-ResY={h}")]),
                Engine::Source => out.extend(["-w".into(), w.to_string(), "-h".into(), h.to_string()]),
                Engine::Godot => out.extend(["--resolution".into(), format!("{w}x{h}")]),
                Engine::Unknown => unsupported(&tr!("Résolution"), notes),
            }
        }
        let display: &[&str] = match (engine, self.display) {
            (_, DisplayMode::Default) => &[],
            (Engine::Unity, DisplayMode::Windowed) => &["-screen-fullscreen", "0"],
            (Engine::Unity, DisplayMode::Fullscreen) => &["-screen-fullscreen", "1", "-window-mode", "exclusive"],
            (Engine::Unity, DisplayMode::Borderless) => &["-screen-fullscreen", "1", "-window-mode", "borderless"],
            (Engine::Unreal, DisplayMode::Windowed) => &["-windowed"],
            (Engine::Unreal, DisplayMode::Fullscreen) => &["-fullscreen"],
            (Engine::Source, DisplayMode::Windowed) => &["-windowed"],
            (Engine::Source, DisplayMode::Fullscreen) => &["-fullscreen"],
            (Engine::Source, DisplayMode::Borderless) => &["-windowed", "-noborder"],
            (Engine::Godot, DisplayMode::Windowed) => &["--windowed"],
            (Engine::Godot, DisplayMode::Fullscreen) => &["--fullscreen"],
            _ => {
                unsupported(&tr!("Mode « {mode} »", mode = self.display.label()), notes);
                &[]
            }
        };
        out.extend(display.iter().map(|s| s.to_string()));
        if let Some(fps) = self.fps_limit {
            match engine {
                Engine::Source => out.extend(["+fps_max".into(), fps.to_string()]),
                Engine::Godot => out.extend(["--max-fps".into(), fps.to_string()]),
                // Ailleurs : variable DXVK sous Wine (voir `plan`), sinon note.
                _ => {}
            }
        }
        out
    }

    /// Construit le plan d'application du profil.
    pub fn plan(&self, base_args: &str, engine: Engine, kind: LaunchKind, host: HostInfo) -> ProfilePlan {
        let mut plan = ProfilePlan::default();
        let mut notes = Vec::new();
        let mut args = self.engine_args(engine, &mut notes);
        args.extend(split_args(&self.args));
        let fps_by_args = matches!(engine, Engine::Source | Engine::Godot);

        if !args.is_empty() || self.replace_args {
            match kind {
                LaunchKind::PlatformClient => {
                    notes.push(tr!("Arguments non transmis : ce client ne permet pas de passer des options au jeu."));
                }
                _ => {
                    let mut all = if self.replace_args { Vec::new() } else { split_args(base_args) };
                    all.extend(args);
                    plan.args = Some(all);
                }
            }
        }
        if kind == LaunchKind::Steam && plan.args.as_ref().is_some_and(|a| !a.is_empty()) {
            notes.push(tr!("Steam demandera de confirmer les arguments au premier lancement."));
        }

        let direct = kind == LaunchKind::Direct;
        let mut env = self.env.clone();
        if let Some(fps) = self.fps_limit.filter(|_| !fps_by_args) {
            if direct && host.via_wine {
                env.push(("DXVK_FRAME_RATE".into(), fps.to_string()));
            } else {
                notes.push(tr!("Limite d'images : non applicable automatiquement ici ; utilisez le réglage du jeu, du pilote graphique, MangoHud ou RTSS."));
            }
        }
        match self.gpu {
            GpuPreference::Default => {}
            pref if host.windows => {
                if !direct {
                    notes.push(tr!("Choix du GPU : uniquement pour les jeux lancés directement par Halcyon."));
                } else if self.allow_gpu_registry {
                    plan.gpu_registry = Some(pref);
                } else {
                    notes.push(tr!("Choix du GPU : autorisation de modifier la préférence graphique Windows requise."));
                }
            }
            GpuPreference::HighPerformance if direct => {
                if host.nvidia {
                    env.push(("__NV_PRIME_RENDER_OFFLOAD".into(), "1".into()));
                    env.push(("__GLX_VENDOR_LIBRARY_NAME".into(), "nvidia".into()));
                    env.push(("__VK_LAYER_NV_optimus".into(), "NVIDIA_only".into()));
                } else {
                    env.push(("DRI_PRIME".into(), "1".into()));
                }
            }
            GpuPreference::PowerSaving if direct => env.push(("DRI_PRIME".into(), "0".into())),
            _ => notes.push(tr!("Choix du GPU : uniquement pour les jeux lancés directement par Halcyon.")),
        }
        if !env.is_empty() {
            if direct {
                plan.env = env;
            } else {
                notes.push(tr!("Variables d'environnement : non transmises au client de la plateforme."));
            }
        }
        if self.priority != ProcessPriority::Normal {
            if direct {
                plan.priority = Some(self.priority);
            } else {
                notes.push(tr!("Priorité : uniquement pour les jeux lancés directement par Halcyon."));
            }
        }
        if let Some(wd) = &self.working_dir {
            if direct {
                plan.working_dir = Some(wd.clone());
            } else {
                notes.push(tr!("Répertoire de travail : géré par le client de la plateforme."));
            }
        }
        plan.notes = notes;
        plan
    }
}

/// URI Steam transmettant des arguments au jeu (`steam://run/<id>//<args>/`).
pub fn steam_run_uri(appid: &str, args: &[String]) -> String {
    let line = args
        .iter()
        .map(|a| if a.contains(' ') { format!("\"{a}\"") } else { a.clone() })
        .collect::<Vec<_>>()
        .join(" ");
    let mut enc = String::new();
    for b in line.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'+' | b'=' => enc.push(b as char),
            _ => enc.push_str(&format!("%{b:02X}")),
        }
    }
    format!("steam://run/{appid}//{enc}/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_specific_arguments() {
        let p = LaunchProfile {
            resolution: Some((1920, 1080)),
            display: DisplayMode::Windowed,
            fps_limit: Some(60),
            args: "-novid".into(),
            ..LaunchProfile::new(1, "Perf")
        };
        let plan = p.plan("-console", Engine::Source, LaunchKind::Direct, HostInfo::default());
        assert_eq!(
            plan.args.unwrap(),
            ["-console", "-w", "1920", "-h", "1080", "-windowed", "+fps_max", "60", "-novid"]
        );
        assert!(plan.notes.is_empty());

        let plan = p.plan("", Engine::Unity, LaunchKind::Direct, HostInfo { via_wine: true, ..Default::default() });
        assert!(plan.args.unwrap().starts_with(&["-screen-width".into(), "1920".into()]));
        assert_eq!(plan.env, [("DXVK_FRAME_RATE".to_string(), "60".to_string())]);

        // Moteur inconnu : rien n'est inventé, tout est signalé.
        let plan = p.plan("", Engine::Unknown, LaunchKind::Direct, HostInfo::default());
        assert_eq!(plan.args.unwrap(), ["-novid"]);
        assert_eq!(plan.notes.len(), 3);
    }

    #[test]
    fn platform_limits_are_reported() {
        let p = LaunchProfile {
            args: "-dx11".into(),
            priority: ProcessPriority::High,
            gpu: GpuPreference::HighPerformance,
            env: vec![("A".into(), "1".into())],
            ..Default::default()
        };
        let epic = p.plan("", Engine::Unreal, LaunchKind::PlatformClient, HostInfo::default());
        assert!(epic.args.is_none() && epic.env.is_empty() && epic.priority.is_none());
        assert_eq!(epic.notes.len(), 4);
        let steam = p.plan("", Engine::Unreal, LaunchKind::Steam, HostInfo { windows: true, ..Default::default() });
        assert_eq!(steam.args.unwrap(), ["-dx11"]);
        let linux = p.plan("", Engine::Unreal, LaunchKind::Direct, HostInfo { nvidia: true, ..Default::default() });
        assert!(linux.env.iter().any(|(k, _)| k == "__NV_PRIME_RENDER_OFFLOAD"));
        assert_eq!(linux.priority, Some(ProcessPriority::High));
        let win = p.plan("", Engine::Unreal, LaunchKind::Direct, HostInfo { windows: true, ..Default::default() });
        assert!(win.gpu_registry.is_none(), "jamais sans autorisation explicite");
        let p2 = LaunchProfile { allow_gpu_registry: true, ..p };
        let win = p2.plan("", Engine::Unreal, LaunchKind::Direct, HostInfo { windows: true, ..Default::default() });
        assert_eq!(win.gpu_registry, Some(GpuPreference::HighPerformance));
    }

    #[test]
    fn steam_uri_encoding() {
        assert_eq!(steam_run_uri("620", &["-novid".into(), "+fps_max 60".into()]), "steam://run/620//-novid%20%22+fps_max%2060%22/");
    }
}
