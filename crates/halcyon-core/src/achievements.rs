//! Format commun des succès, quelle que soit leur origine.
//!
//! Règle absolue : un succès n'est jamais inventé ni estimé. Quand une plateforme
//! ne fournit pas l'information, elle est absente (`None`), pas devinée.

use crate::GameId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AchievementProvider {
    Steam,
    RetroAchievements,
    /// Créé par l'utilisateur pour un jeu sans système de succès : jamais présenté
    /// comme vérifié par une plateforme.
    Custom,
}

impl AchievementProvider {
    pub const ALL: [Self; 3] = [Self::Steam, Self::RetroAchievements, Self::Custom];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Steam => "steam",
            Self::RetroAchievements => "retroachievements",
            Self::Custom => "custom",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == s)
    }

    pub fn label(self) -> String {
        match self {
            Self::Steam => "Steam".into(),
            Self::RetroAchievements => "RetroAchievements".into(),
            Self::Custom => tr!("Personnalisé"),
        }
    }
}

/// Succès tel que fourni par une plateforme, avant stockage.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AchievementData {
    /// Identifiant stable côté plateforme (apiname Steam, ID RetroAchievements…).
    pub api_name: String,
    pub name: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub hidden: bool,
    pub points: Option<u32>,
    /// Pourcentage de joueurs l'ayant obtenu, si la plateforme le publie.
    pub rarity: Option<f32>,
    pub unlocked: bool,
    pub unlocked_at: Option<i64>,
    /// RetroAchievements : déblocage en mode Hardcore.
    pub unlocked_hardcore_at: Option<i64>,
    /// Progression (actuelle, maximum), si la plateforme la fournit.
    pub progress: Option<(f64, f64)>,
    pub display_order: i64,
}

/// Succès stocké localement.
#[derive(Clone, Debug, PartialEq)]
pub struct Achievement {
    pub id: i64,
    pub game_id: GameId,
    pub provider: AchievementProvider,
    pub data: AchievementData,
    /// Icône téléchargée dans le cache local.
    pub icon_path: Option<std::path::PathBuf>,
}

/// Niveau de rareté affiché.
pub fn rarity_label(percent: f32) -> String {
    match percent {
        p if p < 5.0 => tr!("Ultra rare"),
        p if p < 15.0 => tr!("Très rare"),
        p if p < 35.0 => tr!("Rare"),
        p if p < 60.0 => tr!("Peu commun"),
        _ => tr!("Commun"),
    }
}

/// Résumé de progression d'un jeu.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Progress {
    pub unlocked: u32,
    pub total: u32,
}

impl Progress {
    pub fn percent(self) -> f32 {
        if self.total == 0 { 0.0 } else { self.unlocked as f32 * 100.0 / self.total as f32 }
    }

    pub fn complete(self) -> bool {
        self.total > 0 && self.unlocked == self.total
    }
}

/// Résultat d'une synchronisation pour un jeu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncStatus {
    Ok,
    /// Le jeu n'a pas de succès sur cette plateforme.
    NoAchievements,
    /// Le jeu n'est pas reconnu (ex. ROM sans correspondance RetroAchievements).
    NotRecognized,
    /// Steam n'a pas encore enregistré les succès de ce jeu sur ce PC (lecture locale).
    NotCached,
    Error(String),
}

impl SyncStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Ok => "ok",
            Self::NoAchievements => "none",
            Self::NotRecognized => "unknown",
            Self::NotCached => "notcached",
            Self::Error(_) => "error",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_and_rarity() {
        assert_eq!(Progress { unlocked: 3, total: 4 }.percent(), 75.0);
        assert!(Progress { unlocked: 4, total: 4 }.complete());
        assert!(!Progress::default().complete());
        assert_eq!(rarity_label(2.0), "Ultra rare");
        assert_eq!(rarity_label(80.0), "Commun");
        assert_eq!(AchievementProvider::parse("custom"), Some(AchievementProvider::Custom));
    }
}
